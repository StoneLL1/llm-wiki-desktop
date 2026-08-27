import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { ImportMigrationDialog, type ImportMigrationUiStatus } from "../import/ImportMigrationDialog";
import type { ImportWorkflow } from "../import/importWorkflow";
import { selectTaskById, useTaskStore } from "../../stores/taskStore";
import type {
  LegacyInventory,
  MigrationConfirmation,
  MigrationPlan,
  MigrationReport,
} from "../../types/importV2Migration";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";

export interface ImportCompatibilitySettingsProps {
  workflow: ImportWorkflow | null;
}

function migrationStatus(readiness: ImportFrontendReadiness | null): ImportMigrationUiStatus {
  if (!readiness) return "not_scanned";
  if (readiness.active && readiness.migrationStatus === "applied") return "activated";
  if (!readiness.active && readiness.migrationStatus === "applied") return "not_activated";
  return readiness.migrationStatus;
}

export function ImportCompatibilitySettings({ workflow }: ImportCompatibilitySettingsProps) {
  const { t } = useTranslation();
  const readiness = workflow?.readiness ?? null;
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<ImportMigrationUiStatus>(() => migrationStatus(readiness));
  const [inventory, setInventory] = useState<LegacyInventory | null>(null);
  const [plan, setPlan] = useState<MigrationPlan | null>(null);
  const [report, setReport] = useState<MigrationReport | null>(null);
  const [confirmation, setConfirmation] = useState<MigrationConfirmation | null>(null);
  const [migrationTaskId, setMigrationTaskId] = useState<string | null>(null);
  const migrationTaskStatus = useTaskStore((state) =>
    selectTaskById(state, migrationTaskId)?.status ?? null,
  );
  const activeProjectKeyRef = useRef(workflow?.projectKey ?? null);
  activeProjectKeyRef.current = workflow?.projectKey ?? null;

  useEffect(() => {
    setOpen(false);
    setStatus(migrationStatus(readiness));
    setInventory(null);
    setPlan(null);
    setReport(null);
    setConfirmation(null);
    setMigrationTaskId(null);
  }, [readiness, workflow?.projectKey]);

  useEffect(() => {
    if (!workflow || !migrationTaskId || !migrationTaskStatus) return;
    if (migrationTaskStatus === "failed" || migrationTaskStatus === "cancelled") {
      setStatus("resumable");
      setMigrationTaskId(null);
      return;
    }
    if (migrationTaskStatus !== "succeeded") return;
    const requestProjectKey = workflow.projectKey;
    setMigrationTaskId(null);
    void workflow.getMigrationStatus().then((snapshot) => {
      if (activeProjectKeyRef.current !== requestProjectKey || !snapshot) return;
      setStatus(snapshot.status);
      setReport(snapshot.report ?? null);
    }).catch(() => undefined);
  }, [migrationTaskId, migrationTaskStatus, workflow]);

  useEffect(() => {
    if (!open || !workflow) return;
    const requestProjectKey = workflow.projectKey;
    let current = true;
    setStatus(migrationStatus(readiness));
    void workflow.getMigrationStatus().then((snapshot) => {
      if (!current || activeProjectKeyRef.current !== requestProjectKey || !snapshot) return;
      setStatus(snapshot.status);
      setReport(snapshot.report ?? null);
    }).catch(() => undefined);
    return () => { current = false; };
  }, [open, readiness, workflow]);

  async function scanMigration() {
    if (!workflow) return;
    const requestProjectKey = workflow.projectKey;
    setStatus("scanning");
    const next = await workflow.scanMigration();
    if (activeProjectKeyRef.current !== requestProjectKey) return;
    if (!next) {
      setStatus(migrationStatus(readiness));
      return;
    }
    setInventory(next);
    setPlan(null);
    setReport(null);
    setConfirmation(null);
    setStatus("dry_run_ready");
  }

  async function buildPlan(nextInventory: LegacyInventory) {
    if (!workflow) return;
    const requestProjectKey = workflow.projectKey;
    setStatus("scanning");
    const preparation = await workflow.planMigration(nextInventory);
    if (activeProjectKeyRef.current !== requestProjectKey) return;
    if (!preparation) {
      setStatus(migrationStatus(readiness));
      return;
    }
    setPlan(preparation.plan);
    setReport(preparation.report);
    setConfirmation(preparation.confirmation);
    setStatus("awaiting_confirmation");
  }

  const unavailable = !workflow || Boolean(workflow.readinessWarning);
  const statusKey = `importV2.migration.status.${status}`;

  return (
    <section className="settings-view__section">
      <div>
        <h2 className="settings-view__section-title">{t("settings.compatibility.title")}</h2>
        <p className="settings-view__section-desc">{t("settings.compatibility.description")}</p>
      </div>
      <div className="settings-view__cards">
        <div className="settings-view__card">
          <div className="settings-view__card-label">{t("settings.compatibility.importData")}</div>
          <div className="settings-view__card-value">{t(statusKey, { defaultValue: status })}</div>
          <div className="mt-3">
            <button
              type="button"
              className="btn btn--sm"
              disabled={unavailable}
              onClick={() => setOpen(true)}
            >
              {t("settings.compatibility.review")}
            </button>
          </div>
        </div>
      </div>
      {workflow ? (
        <ImportMigrationDialog
          open={open}
          status={status}
          inventory={inventory}
          plan={plan}
          report={report}
          confirmation={confirmation}
          checkpoint={null}
          resumable={status === "interrupted" || status === "resumable" || status === "applying"}
          onScan={scanMigration}
          onPlan={buildPlan}
          onApply={async (nextPlan, confirmation) => {
            setStatus("applying");
            try {
              const task = await workflow.applyMigration(nextPlan, confirmation);
              if (task) setMigrationTaskId(task.id);
              else setStatus("awaiting_confirmation");
            } catch (error) {
              setStatus("awaiting_confirmation");
              throw error;
            }
          }}
          onResume={async (nextPlan, confirmation) => {
            setStatus("applying");
            try {
              const task = await workflow.resumeMigration(nextPlan, confirmation);
              if (task) setMigrationTaskId(task.id);
              else setStatus("resumable");
            } catch (error) {
              setStatus("resumable");
              throw error;
            }
          }}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </section>
  );
}
