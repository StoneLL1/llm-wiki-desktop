import { GitBranch, RefreshCw, ShieldCheck, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useModalDialog } from "../../hooks/useModalDialog";
import { useProjectStore } from "../../stores/projectStore";
import type { ProjectOpenAssessment, ProjectSummary, ProjectTemplate } from "../../types/project";
import { ProjectAssessmentPanel } from "./ProjectAssessmentPanel";

export type ProjectAuthorityAction =
  | "manage"
  | "open_or_create_project"
  | "trust_project"
  | "make_writable"
  | "configure_git"
  | "resolve_dirty_git"
  | "repair_project";

interface ProjectAuthorityDialogProps {
  action: ProjectAuthorityAction;
  project: Pick<ProjectSummary, "projectId" | "rootPath">;
  onClose: () => void;
  onSatisfied: () => Promise<void> | void;
}

function isCompatibleCandidate(assessment: ProjectOpenAssessment): boolean {
  return assessment.capabilities.includes("enable_compatible_features");
}

function satisfies(action: ProjectAuthorityAction, assessment: ProjectOpenAssessment): boolean {
  if (action === "trust_project") return assessment.trust === "trusted";
  if (action === "configure_git") return assessment.git.head != null;
  if (action === "resolve_dirty_git") return assessment.git.head != null && !assessment.git.hasChanges;
  if (action === "make_writable") return assessment.filesystemAccess === "writable";
  if (action === "repair_project") return !assessment.repairAvailable;
  return false;
}

export function ProjectAuthorityDialog({
  action,
  project,
  onClose,
  onSatisfied,
}: ProjectAuthorityDialogProps) {
  const { t } = useTranslation();
  const closeRef = useRef<HTMLButtonElement>(null);
  const dialogRef = useModalDialog<HTMLDivElement>({ open: true, onClose, initialFocusRef: closeRef });
  const currentProject = useProjectStore((state) => state.currentProject);
  const assessment = useProjectStore((state) => state.assessment);
  const assessing = useProjectStore((state) => state.assessing);
  const assessmentError = useProjectStore((state) => state.assessmentError);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const assessCurrentProject = useProjectStore((state) => state.assessCurrentProject);
  const trustProject = useProjectStore((state) => state.trustAssessedProject);
  const revokeTrust = useProjectStore((state) => state.revokeAssessedProjectTrust);
  const enableCompatible = useProjectStore((state) => state.enableCompatibleFullFeatures);
  const repairProject = useProjectStore((state) => state.repairAssessedProject);
  const initializeGit = useProjectStore((state) => state.requestAssessedGitInitialization);
  const checkpointGit = useProjectStore((state) => state.requestAssessedGitCheckpoint);
  const [initializeGitWithCompatibility, setInitializeGitWithCompatibility] = useState(true);
  const [template] = useState<ProjectTemplate>(currentProject.template);
  const [pendingAuthorityActionId, setPendingAuthorityActionId] = useState<string | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const initiatingProjectKey = useRef(`${project.projectId}\0${project.rootPath}`);

  const isInitiatingProjectCurrent = useCallback(() => {
    const project = useProjectStore.getState().currentProject;
    return initiatingProjectKey.current === `${project.projectId}\0${project.rootPath}`;
  }, []);

  useEffect(() => {
    if (!isInitiatingProjectCurrent()) {
      onClose();
      return;
    }
    void assessCurrentProject()
      .then(async (nextAssessment) => {
        if (
          isInitiatingProjectCurrent()
          && action !== "manage"
          && satisfies(action, nextAssessment)
        ) {
          await onSatisfied();
          onClose();
        }
      })
      .catch((error) => setLocalError(String(error)));
  }, [
    action,
    assessCurrentProject,
    currentProject.projectId,
    currentProject.rootPath,
    isInitiatingProjectCurrent,
    onClose,
    onSatisfied,
  ]);

  useEffect(() => {
    if (!pendingAuthorityActionId || pendingAction?.id === pendingAuthorityActionId) return;
    setPendingAuthorityActionId(null);
    void assessCurrentProject()
      .then(async (nextAssessment) => {
        if (
          !isInitiatingProjectCurrent()
          || nextAssessment.canonicalIdentityKey !== assessment?.canonicalIdentityKey
        ) {
          return;
        }
        if (satisfies(action, nextAssessment)) {
          await onSatisfied();
          onClose();
          return;
        }
        if (action === "manage") return;
        setLocalError(t("projectAuthority.confirmationNotApplied"));
      })
      .catch((error) => setLocalError(String(error)));
  }, [action, assessCurrentProject, assessment?.canonicalIdentityKey, isInitiatingProjectCurrent, onClose, onSatisfied, pendingAction?.id, pendingAuthorityActionId, t]);

  const requestConfirmation = async (request: () => Promise<void>) => {
    setLocalError(null);
    try {
      if (!isInitiatingProjectCurrent()) {
        onClose();
        return;
      }
      await request();
      const actionId = useProjectStore.getState().pendingAction?.id;
      if (actionId) setPendingAuthorityActionId(actionId);
    } catch (error) {
      setLocalError(String(error));
    }
  };

  const refreshAndContinue = async () => {
    setLocalError(null);
    try {
      if (!isInitiatingProjectCurrent()) {
        onClose();
        return;
      }
      const nextAssessment = await assessCurrentProject();
      if (satisfies(action, nextAssessment)) {
        await onSatisfied();
        onClose();
      }
    } catch (error) {
      setLocalError(String(error));
    }
  };

  const revoke = async (assessmentId: string) => {
    setLocalError(null);
    try {
      if (!isInitiatingProjectCurrent()) {
        onClose();
        return;
      }
      await revokeTrust(assessmentId);
    } catch (error) {
      setLocalError(String(error));
    }
  };

  return (
    <div
      ref={dialogRef}
      aria-labelledby="project-authority-title"
      aria-modal="true"
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      tabIndex={-1}
    >
      <section className="max-h-[82vh] w-full max-w-[800px] overflow-y-auto border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <ShieldCheck aria-hidden="true" className="text-[var(--accent)]" size={17} />
          <div className="min-w-0">
            <h2 className="m-0 text-[15px] font-semibold" id="project-authority-title">
              {t("projectAuthority.title")}
            </h2>
            <p className="m-0 mt-0.5 text-[11px] text-[var(--text-muted)]">
              {t(`projectAuthority.action.${action}`)}
            </p>
          </div>
          <button ref={closeRef} aria-label={t("projectAuthority.close")} className="icon-button ml-auto" onClick={onClose} type="button">
            <X aria-hidden="true" size={16} />
          </button>
        </header>

        {assessing && !assessment ? (
          <div className="px-4 py-8 text-center text-[12px] text-[var(--text-muted)]" role="status">
            {t("projectAssessment.scanning")}
          </div>
        ) : assessment ? (
          <>
            <ProjectAssessmentPanel assessment={assessment} onBack={onClose} />
            <div className="mx-auto mb-5 mt-3 w-full max-w-[760px] border border-[var(--border)] bg-[var(--surface)] px-4 py-3">
              {action === "trust_project" && assessment.trust === "untrusted" ? (
                <div className="space-y-3">
                  <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("projectAuthority.trustDescription")}</p>
                  <div className="flex flex-wrap items-center gap-2">
                    <button className="btn btn--secondary" onClick={() => void requestConfirmation(() => trustProject(assessment.assessmentId))} type="button">
                      {t("projectAuthority.trustOnly")}
                    </button>
                    {isCompatibleCandidate(assessment) ? (
                      <button className="btn btn--primary" onClick={() => void requestConfirmation(() => enableCompatible(assessment.assessmentId, template, initializeGitWithCompatibility))} type="button">
                        {t("projectAuthority.enableCompatible")}
                      </button>
                    ) : null}
                    {isCompatibleCandidate(assessment) ? (
                      <label className="ml-auto flex items-center gap-2 text-[12px] text-[var(--text-secondary)]">
                        <input checked={initializeGitWithCompatibility} onChange={(event) => setInitializeGitWithCompatibility(event.target.checked)} type="checkbox" />
                        {t("projectAuthority.initializeGit")}
                      </label>
                    ) : null}
                  </div>
                </div>
              ) : null}

              {action === "configure_git" && assessment.git.head == null ? (
                <button className="btn btn--primary" onClick={() => void requestConfirmation(() => initializeGit(assessment.assessmentId))} type="button">
                  <GitBranch aria-hidden="true" size={14} />
                  {t("projectAuthority.configureGit")}
                </button>
              ) : null}

              {action === "resolve_dirty_git" ? (
                <div className="space-y-3">
                  <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("projectAuthority.dirtyDescription")}</p>
                  <div className="flex flex-wrap gap-2">
                    {assessment.git.head == null ? (
                      <button className="btn btn--primary" onClick={() => void requestConfirmation(() => initializeGit(assessment.assessmentId))} type="button">
                        <GitBranch aria-hidden="true" size={14} />
                        {t("projectAuthority.configureGit")}
                      </button>
                    ) : (
                      <button className="btn btn--primary" onClick={() => void requestConfirmation(() => checkpointGit(assessment.assessmentId))} type="button">
                        {t("projectAuthority.createCheckpoint")}
                      </button>
                    )}
                    <button className="btn btn--secondary" onClick={() => void refreshAndContinue()} type="button">
                      <RefreshCw aria-hidden="true" size={14} />
                      {t("projectAuthority.refresh")}
                    </button>
                  </div>
                </div>
              ) : null}

              {action === "make_writable" ? (
                <div className="space-y-3">
                  <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("projectAuthority.readOnlyDescription")}</p>
                  <button className="btn btn--secondary" onClick={() => void refreshAndContinue()} type="button">
                    <RefreshCw aria-hidden="true" size={14} />
                    {t("projectAuthority.refresh")}
                  </button>
                </div>
              ) : null}

              {action === "repair_project" ? (
                <div className="space-y-3">
                  <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("projectAuthority.repairDescription")}</p>
                  {assessment.repairAvailable ? (
                    <button className="btn btn--primary" onClick={() => void requestConfirmation(() => repairProject(assessment.assessmentId))} type="button">
                      {t("projectAuthority.repairProject")}
                    </button>
                  ) : (
                    <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("projectAuthority.repairUnavailable")}</p>
                  )}
                </div>
              ) : null}

              {action === "manage" ? (
                <div className="space-y-3">
                  <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("projectAuthority.manageDescription")}</p>
                  <div className="flex flex-wrap items-center gap-2">
                    {assessment.trust === "trusted" ? (
                      assessment.format !== "native_current" ? (
                        <button className="btn btn--secondary" onClick={() => void revoke(assessment.assessmentId)} type="button">
                          {t("projectAuthority.revokeTrust")}
                        </button>
                      ) : null
                    ) : (
                      <button className="btn btn--secondary" onClick={() => void requestConfirmation(() => trustProject(assessment.assessmentId))} type="button">
                        {t("projectAuthority.trustOnly")}
                      </button>
                    )}
                    {isCompatibleCandidate(assessment) ? (
                      <>
                        <button className="btn btn--primary" onClick={() => void requestConfirmation(() => enableCompatible(assessment.assessmentId, template, initializeGitWithCompatibility))} type="button">
                          {t("projectAuthority.enableCompatible")}
                        </button>
                        <label className="flex items-center gap-2 text-[12px] text-[var(--text-secondary)]">
                          <input checked={initializeGitWithCompatibility} onChange={(event) => setInitializeGitWithCompatibility(event.target.checked)} type="checkbox" />
                          {t("projectAuthority.initializeGit")}
                        </label>
                      </>
                    ) : null}
                    {assessment.git.head == null ? (
                      <button className="btn btn--secondary" onClick={() => void requestConfirmation(() => initializeGit(assessment.assessmentId))} type="button">
                        <GitBranch aria-hidden="true" size={14} />
                        {t("projectAuthority.configureGit")}
                      </button>
                    ) : assessment.git.hasChanges ? (
                      <button className="btn btn--secondary" onClick={() => void requestConfirmation(() => checkpointGit(assessment.assessmentId))} type="button">
                        {t("projectAuthority.createCheckpoint")}
                      </button>
                    ) : null}
                    {assessment.repairAvailable ? (
                      <button className="btn btn--primary" onClick={() => void requestConfirmation(() => repairProject(assessment.assessmentId))} type="button">
                        {t("projectAuthority.repairProject")}
                      </button>
                    ) : null}
                  </div>
                </div>
              ) : null}

              {action === "open_or_create_project" ? <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("projectAuthority.openProjectUnavailable")}</p> : null}
              {pendingAuthorityActionId ? <p className="m-0 mt-3 text-[11px] text-[var(--text-muted)]">{t("projectAuthority.awaitingConfirmation")}</p> : null}
              {localError || assessmentError ? <p className="m-0 mt-3 text-[12px] text-[var(--danger)]" role="alert">{localError ?? assessmentError}</p> : null}
            </div>
          </>
        ) : (
          <div className="px-4 py-8 text-[12px] text-[var(--danger)]" role="alert">{localError ?? assessmentError}</div>
        )}
      </section>
    </div>
  );
}
