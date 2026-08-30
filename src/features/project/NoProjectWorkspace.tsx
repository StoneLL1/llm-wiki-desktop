import { FolderOpen, FolderPlus, Package, ShieldAlert } from "lucide-react";
import { lazy, Suspense, useState } from "react";
import { useTranslation } from "react-i18next";

import { LazyActionableErrorNotice } from "../../components/app/LazyActionableErrorNotice";
import {
  normalizeBackendError,
  type NormalizedBackendError,
} from "../../lib/backendError";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { pickDirectory } from "../import/nativeFilePicker";
import type { NewProjectPayload } from "./NewProjectDialog";

const NewProjectDialog = lazy(async () => {
  const module = await import("./NewProjectDialog");
  return { default: module.NewProjectDialog };
});

const ProjectAssessmentPanel = lazy(async () => {
  const module = await import("./ProjectAssessmentPanel");
  return { default: module.ProjectAssessmentPanel };
});

function canOpenAssessment(assessment: ReturnType<typeof useProjectStore.getState>["assessment"]): boolean {
  return Boolean(
    assessment
    && !["ambiguous_markdown", "ordinary_materials", "unknown"].includes(assessment.format)
    && assessment.health !== "unreadable",
  );
}

export function NoProjectWorkspace({ activeView }: { activeView: string }) {
  const { t } = useTranslation();
  const assessment = useProjectStore((state) => state.assessment);
  const assessing = useProjectStore((state) => state.assessing);
  const assessmentError = useProjectStore((state) => state.assessmentError);
  const assessProject = useProjectStore((state) => state.assessProject);
  const cancelProjectAssessment = useProjectStore((state) => state.cancelProjectAssessment);
  const openAssessedProject = useProjectStore((state) => state.openAssessedProject);
  const resolveAmbiguousAssessedProject = useProjectStore((state) => state.resolveAmbiguousAssessedProject);
  const rememberAmbiguousProjectIntent = useProjectStore((state) => state.rememberAmbiguousProjectIntent);
  const clearAmbiguousProjectIntent = useProjectStore((state) => state.clearAmbiguousProjectIntent);
  const createProject = useProjectStore((state) => state.createProject);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const setImportSuccessNotice = useNavigationStore((state) => state.setImportSuccessNotice);
  const setPendingImportPath = useNavigationStore((state) => state.setPendingImportPath);
  const [newDialogOpen, setNewDialogOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState<NormalizedBackendError | null>(null);
  const [materialsPath, setMaterialsPath] = useState<string | null>(null);

  const openExisting = async () => {
    setLocalError(null);
    try {
      const selected = await pickDirectory({ title: t("noProject.open.picker") });
      if (!selected) return;
      setBusy(true);
      const nextAssessment = await assessProject(selected);
      if (canOpenAssessment(nextAssessment)) {
        await openAssessedProject(nextAssessment.assessmentId);
        setActiveView("dashboard");
      } else if (
        nextAssessment.format === "ambiguous_markdown"
        && nextAssessment.rememberedOpenIntent === "open_as_markdown_vault"
      ) {
        await resolveAmbiguousAssessedProject(nextAssessment.assessmentId);
        setActiveView("dashboard");
      }
    } catch (error) {
      setLocalError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.project",
        defaultActionKind: "retry",
        defaultRecoverable: true,
      }));
    } finally {
      setBusy(false);
    }
  };

  const create = async (payload: NewProjectPayload) => {
    setLocalError(null);
    setBusy(true);
    try {
      const project = await createProject(payload);
      setNewDialogOpen(false);
      setImportSuccessNotice({ projectId: project.projectId, name: project.name });
      if (materialsPath) {
        setPendingImportPath({ projectId: project.projectId, path: materialsPath });
      }
      setMaterialsPath(null);
      setActiveView("import");
    } catch (error) {
      setLocalError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.project",
        defaultActionKind: "retry",
        defaultRecoverable: true,
      }));
    } finally {
      setBusy(false);
    }
  };

  const createFromAssessment = async () => {
    if (!assessment) return;
    setLocalError(null);
    try {
      if (assessment.format === "ambiguous_markdown") {
        await rememberAmbiguousProjectIntent(assessment.assessmentId, "create_from_materials");
      }
      setMaterialsPath(assessment.canonicalRootPath);
      setNewDialogOpen(true);
    } catch (error) {
      setLocalError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.project",
        defaultActionKind: "retry",
        defaultRecoverable: true,
      }));
    }
  };

  const openAmbiguousAssessment = async () => {
    if (!assessment) return;
    setLocalError(null);
    setBusy(true);
    try {
      await resolveAmbiguousAssessedProject(assessment.assessmentId);
      setActiveView("dashboard");
    } catch (error) {
      setLocalError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.project",
        defaultActionKind: "retry",
        defaultRecoverable: true,
      }));
    } finally {
      setBusy(false);
    }
  };

  const clearAmbiguousAssessmentIntent = async () => {
    if (!assessment) return;
    setLocalError(null);
    setBusy(true);
    try {
      await clearAmbiguousProjectIntent(assessment.assessmentId);
    } catch (error) {
      setLocalError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.project",
        defaultActionKind: "retry",
        defaultRecoverable: true,
      }));
    } finally {
      setBusy(false);
    }
  };

  if (assessing) {
    return (
      <div className="no-project-workspace" aria-live="polite">
        <section className="no-project-assessment-state">
          <span className="dotstatus dotstatus--busy" aria-hidden="true" />
          <div>
            <h2>{t("projectAssessment.scanning")}</h2>
            <p>{t("projectAssessment.scanningDetail")}</p>
          </div>
          <button className="btn btn--secondary ml-auto" onClick={() => void cancelProjectAssessment()} type="button">
            {t("projectAssessment.cancel")}
          </button>
        </section>
      </div>
    );
  }

  if (assessment) {
    const isMaterials = assessment.format === "ordinary_materials";
    const isAmbiguous = assessment.format === "ambiguous_markdown";
    return (
      <div className="no-project-workspace">
        <Suspense fallback={null}>
          <ProjectAssessmentPanel assessment={assessment} onBack={() => void cancelProjectAssessment()} />
        </Suspense>
        {isMaterials || isAmbiguous ? (
          <section className="no-project-decision" aria-live="polite">
            <ShieldAlert aria-hidden="true" size={16} />
            <div>
              <h2>{t(isMaterials ? "noProject.materials.title" : "noProject.ambiguous.title")}</h2>
              <p>{t(isMaterials ? "noProject.materials.detail" : "noProject.ambiguous.detail")}</p>
              {isAmbiguous && assessment.rememberedOpenIntent ? (
                <p className="mt-2 text-[12px] text-[var(--text-muted)]">
                  {t("noProject.ambiguous.remembered")}
                </p>
              ) : null}
              <div className="mt-3 flex flex-wrap gap-2">
                {isAmbiguous ? (
                  <button className="btn btn--secondary" disabled={busy} onClick={() => void openAmbiguousAssessment()} type="button">
                    {t("noProject.ambiguous.open")}
                  </button>
                ) : null}
                <button
                  className="btn btn--primary"
                  disabled={busy}
                  onClick={() => void createFromAssessment()}
                  type="button"
                >
                  <FolderPlus aria-hidden="true" size={14} />
                  {t("noProject.materials.create")}
                </button>
                {isAmbiguous && assessment.rememberedOpenIntent ? (
                  <button className="btn btn--ghost" disabled={busy} onClick={() => void clearAmbiguousAssessmentIntent()} type="button">
                    {t("noProject.ambiguous.clear")}
                  </button>
                ) : null}
              </div>
            </div>
          </section>
        ) : null}
        {assessment.health === "unreadable" ? (
          <p className="no-project-error" role="alert">{t("noProject.unreadable")}</p>
        ) : null}
        {localError || assessmentError ? (
          <LazyActionableErrorNotice className="no-project-error" error={localError ?? assessmentError} />
        ) : null}
        {newDialogOpen ? (
          <Suspense fallback={null}>
            <NewProjectDialog
              busy={busy}
              error={localError}
              onClose={() => {
                setLocalError(null);
                setNewDialogOpen(false);
              }}
              onCreate={(payload) => void create(payload)}
            />
          </Suspense>
        ) : null}
      </div>
    );
  }

  return (
    <div className="no-project-workspace">
      {activeView === "dashboard" ? (
        <div className="no-project-actions" aria-labelledby="no-project-actions-title">
          <button className="no-project-action" onClick={() => setNewDialogOpen(true)} type="button">
            <span className="no-project-action__icon"><FolderPlus aria-hidden="true" size={18} /></span>
            <span>
              <strong>{t("noProject.new.title")}</strong>
              <small>{t("noProject.new.detail")}</small>
            </span>
          </button>
          <button className="no-project-action" disabled={busy} onClick={() => void openExisting()} type="button">
            <span className="no-project-action__icon"><FolderOpen aria-hidden="true" size={18} /></span>
            <span>
              <strong>{t("noProject.open.title")}</strong>
              <small>{t("noProject.open.detail")}</small>
            </span>
          </button>
          <button
            className="btn btn--ghost no-project-capability-link"
            data-open-capability-management="true"
            onClick={() => {
              void import("../../stores/appCapabilityStore").then(({ useAppCapabilityStore }) => {
                useAppCapabilityStore.getState().openManagement();
              });
            }}
            type="button"
          >
            <Package aria-hidden="true" size={14} />
            {t("importV2.capabilityManagement.noProjectAction")}
          </button>
          <p className="no-project-storage-note">{t("noProject.storageNote")}</p>
        </div>
      ) : (
        <section className="no-project-dependency" aria-labelledby="no-project-dependency-title">
          <h2 id="no-project-dependency-title">{t("noProject.dependency.title")}</h2>
          <p>{t("noProject.dependency.detail", { module: t(`nav.${activeView}`) })}</p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button className="btn btn--primary" onClick={() => setNewDialogOpen(true)} type="button">
              <FolderPlus aria-hidden="true" size={14} />
              {t("noProject.new.title")}
            </button>
            <button className="btn btn--secondary" disabled={busy} onClick={() => void openExisting()} type="button">
              <FolderOpen aria-hidden="true" size={14} />
              {t("noProject.open.title")}
            </button>
          </div>
        </section>
      )}
      {localError || assessmentError ? (
          <LazyActionableErrorNotice className="no-project-error" error={localError ?? assessmentError} />
      ) : null}
      {newDialogOpen ? (
        <Suspense fallback={null}>
          <NewProjectDialog
            busy={busy}
            error={localError}
            onClose={() => {
              setLocalError(null);
              setNewDialogOpen(false);
            }}
            onCreate={(payload) => void create(payload)}
          />
        </Suspense>
      ) : null}
    </div>
  );
}
