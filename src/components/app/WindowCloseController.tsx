import { listen } from "@tauri-apps/api/event";
import { Monitor, Power, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";

import { useWikiStore } from "../../features/wiki/wikiStore";
import { useModalDialog } from "../../hooks/useModalDialog";
import { getSettingsDraftDirty, useSettingsDraftDirty } from "../../lib/settingsDraftGuard";
import { getWindowCloseState, resolveWindowClose, type WindowCloseSnapshot } from "../../services/windowApi";
import { mergeTaskSnapshot, useTaskStore } from "../../stores/taskStore";
import { useWorkflowStore } from "../../stores/workflowStore";
import { TASK_TYPE_LABELS, type BackendTask } from "../../types/task";
import { Button } from "../ui/button";

function hasUnsavedEditor() {
  const wiki = useWikiStore.getState();
  return wiki.saveState === "conflict"
    || (wiki.page !== null && wiki.draft !== wiki.page.rawMarkdown);
}

function activeTasks(snapshot: WindowCloseSnapshot | null, facts: Record<string, BackendTask>) {
  const tasks = new Map(Object.values(facts).map((task) => [task.id, task]));
  for (const task of snapshot?.activeTasks ?? []) {
    tasks.set(task.id, mergeTaskSnapshot(tasks.get(task.id), task).task);
  }
  return [...tasks.values()].filter((task) => ["queued", "running", "cancelling", "waiting_for_confirmation"].includes(task.status));
}

export function WindowCloseController() {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<WindowCloseSnapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [discard, setDiscard] = useState(false);
  const busyRef = useRef(false);
  const requestEpoch = useRef(0);
  const autoQuitAttempted = useRef(false);
  const mounted = useRef(false);
  const taskFacts = useTaskStore((state) => state.taskById);
  const workflowRuns = useWorkflowStore((state) => state.runs);
  const editorSaving = useWikiStore((state) => state.saveState === "saving");
  const unsavedEditor = useWikiStore((state) => state.saveState === "conflict"
    || (state.page !== null && state.draft !== state.page.rawMarkdown));
  const settingsDirty = useSettingsDraftDirty();
  const unsavedChanges = unsavedEditor || settingsDirty;
  const tasks = activeTasks(snapshot, taskFacts);
  const criticalTask = tasks.some((task) => task.status !== "waiting_for_confirmation"
    && (!task.cancellable || task.operation?.kind === "import_commit"))
    || workflowRuns.some((run) => run.displayStatus === "running"
      && (run.currentStageId === "apply_changes" || run.currentStageId?.startsWith("apply_changes_")));
  const open = snapshot?.intent !== undefined && snapshot.intent !== null;

  async function resolve(action: "cancel" | "background" | "quit", discardChanges = discard) {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    requestEpoch.current += 1;
    try {
      await resolveWindowClose({
        action,
        unsavedChanges: hasUnsavedEditor() || getSettingsDraftDirty(),
        discardUnsavedChanges: action === "quit" && discardChanges,
        editorSaving: useWikiStore.getState().saveState === "saving",
      });
      if (!mounted.current) return;
      setSnapshot(null);
      setDiscard(false);
      autoQuitAttempted.current = false;
    } catch (failure) {
      if (!mounted.current) return;
      const code = typeof failure === "object" && failure !== null && "code" in failure ? String(failure.code) : "";
      setError(["APP_CLOSE_EDITOR_SAVING", "APP_CLOSE_CRITICAL_TASK", "UPDATE_INSTALL_GUARD_BLOCKED", "UPDATE_INSTALL_IN_PROGRESS"].includes(code)
        ? "windowClose.waitForOperation"
        : code === "APP_CLOSE_UNSAVED_CHANGES" ? "windowClose.confirmDiscard" : "windowClose.failed");
    } finally {
      busyRef.current = false;
      if (mounted.current) setBusy(false);
    }
  }

  const dialogRef = useModalDialog({ open, onClose: () => { void resolve("cancel"); } });

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    mounted.current = true;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const refresh = async () => {
      if (busyRef.current) return;
      const epoch = ++requestEpoch.current;
      const next = await getWindowCloseState();
      if (!disposed && epoch === requestEpoch.current) setSnapshot(next.intent ? next : null);
    };
    void listen("app://close-requested", () => { void refresh().catch(() => undefined); })
      .then(async (stop) => {
        if (disposed) { stop(); return; }
        unlisten = stop;
        // Recover a native close request received before this controller loaded.
        await refresh();
      }).catch(() => undefined);
    return () => {
      disposed = true;
      mounted.current = false;
      requestEpoch.current += 1;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (snapshot?.intent !== "quit" || autoQuitAttempted.current) return;
    autoQuitAttempted.current = true;
    if (!unsavedChanges && !editorSaving && tasks.length === 0 && !criticalTask) {
      void resolve("quit", false);
    }
    // A quit preference gets one attempt per close request, never an automatic
    // retry after a guard failure or after the user starts reviewing the dialog.
  }, [snapshot?.intent]);

  if (!open) return null;
  const cannotQuit = busy || editorSaving || criticalTask || (unsavedChanges && !discard);
  return createPortal(
    <div ref={dialogRef} className="dialog-overlay" role="dialog" aria-modal="true" aria-labelledby="window-close-title" aria-describedby="window-close-description" tabIndex={-1}>
      <section className="dialog">
        <header className="dialog__head">
          <Power aria-hidden="true" size={17} className="text-[var(--text-muted)]" />
          <h2 className="dialog__title m-0" id="window-close-title">{t("windowClose.title")}</h2>
          <button className="btn btn--ghost btn--icon btn--sm ml-auto" type="button" disabled={busy} aria-label={t("windowClose.cancel")} title={t("windowClose.cancel")} onClick={() => { void resolve("cancel"); }}><X aria-hidden="true" size={16} /></button>
        </header>
        <div className="dialog__body space-y-3">
          <p id="window-close-description" className="m-0 text-[13px] leading-5 text-[var(--text-secondary)]">{t("windowClose.description")}</p>
          <section className="border-y border-[var(--border-subtle)] py-3" aria-labelledby="window-close-tasks">
            <h3 id="window-close-tasks" className="m-0 text-[12px] font-semibold">{t("windowClose.tasks", { count: tasks.length })}</h3>
            {tasks.length > 0 ? <>
              <ul className="m-0 mt-2 max-h-[160px] list-none space-y-2 overflow-y-auto p-0">
                {tasks.map((task) => <li className="flex min-w-0 items-center justify-between gap-3 text-[12px]" key={task.id}><span className="truncate">{task.title || t(TASK_TYPE_LABELS[task.taskType])}</span><span className="shrink-0 text-[var(--text-muted)]">{t(`task.status.${task.status}`)}</span></li>)}
              </ul>
              <p className="m-0 mt-2 text-[12px] leading-5 text-[var(--text-muted)]">{t("windowClose.tasksHint")}</p>
            </> : <p className="m-0 mt-1 text-[12px] text-[var(--text-muted)]">{t("windowClose.noTasks")}</p>}
          </section>
          {unsavedChanges ? <div className="space-y-2">
            <p className="m-0 text-[12px] leading-5 text-[var(--text-secondary)]">{t("windowClose.unsavedChanges")}</p>
            <label className="flex items-start gap-2 text-[12px] leading-5"><input type="checkbox" checked={discard} disabled={busy} onChange={(event) => setDiscard(event.target.checked)} className="mt-1 accent-[var(--accent)]" />{t("windowClose.discard")}</label>
          </div> : null}
          {editorSaving || criticalTask ? <p className="m-0 text-[12px] leading-5 text-[var(--warning)]" role="status">{t("windowClose.waitForOperation")}</p> : null}
          {error ? <p className="m-0 text-[12px] leading-5 text-[var(--danger)]" role="alert">{t(error)}</p> : null}
        </div>
        <footer className="dialog__foot">
          <Button disabled={busy} onClick={() => { void resolve("cancel"); }}>{t("windowClose.cancel")}</Button>
          <Button disabled={cannotQuit} variant="ghost" onClick={() => { void resolve("quit"); }}>{t("windowClose.quit")}</Button>
          <Button disabled={busy} variant="primary" onClick={() => { void resolve("background"); }}><Monitor aria-hidden="true" size={14} />{t("windowClose.background")}</Button>
        </footer>
      </section>
    </div>, document.body,
  );
}
