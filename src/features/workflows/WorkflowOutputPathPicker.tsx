import { FolderOpen, LoaderCircle } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useProjectStore } from "../../stores/projectStore";
import { captureWorkflowRequestGuard, useWorkflowStore, workflowRequestGuardMatches } from "../../stores/workflowStore";
import type { ProjectSummary } from "../../types/project";
import { pickWorkflowOutputPath } from "./workflowOutputPicker";

export function WorkflowOutputPathPicker({ value, onChange, project: owner }: {
  value: string;
  onChange: (value: string) => void;
  project?: Pick<ProjectSummary, "projectId" | "rootPath">;
}) {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const authority = useProjectStore((state) => state.authority);
  const requestEpoch = useWorkflowStore((state) => state.requestEpoch);
  const project = owner ?? currentProject;
  const matchingAuthority = authority?.projectId === project.projectId ? authority : null;
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  const active = useRef(false);
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const labelId = useId();
  const descriptionId = useId();
  useEffect(() => {
    active.current = true;
    setPending(false);
    setError(null);
    return () => { active.current = false; request.current += 1; };
  }, [project.projectId, project.rootPath, matchingAuthority?.authorityRevision, requestEpoch]);

  const choose = async () => {
    if (pending) return;
    const id = ++request.current;
    const guard = captureWorkflowRequestGuard();
    const current = () => active.current && id === request.current && workflowRequestGuardMatches(guard);
    setPending(true);
    setError(null);
    try {
      const path = await pickWorkflowOutputPath({
        projectRoot: matchingAuthority?.canonicalRootPath ?? project.rootPath,
        exportRoot: matchingAuthority?.layout.exportRoot,
        currentPath: value,
        title: t("workflows.outputPicker.title"),
      });
      if (current() && path !== null) onChangeRef.current(path);
    } catch (cause) {
      if (current()) {
        const code = cause instanceof Error ? cause.message : "failed";
        setError(["outsideExportRoot", "htmlOnly", "exportRootUnavailable"].includes(code) ? code : "failed");
      }
    } finally {
      if (current()) setPending(false);
    }
  };

  return <div className="workflow-field">
    <span id={labelId}>{t("workflows.preparation.outputPath")}</span>
    <button type="button" className="workflow-output-picker" aria-labelledby={labelId} aria-describedby={descriptionId} aria-busy={pending} disabled={pending} onClick={() => void choose()}>
      {pending ? <LoaderCircle size={17} aria-hidden="true" /> : <FolderOpen size={17} aria-hidden="true" />}
      <span className="workflow-output-picker__value" id={descriptionId} title={value || undefined}>
        <strong>{value ? value.split(/[\\/]/).pop() : t("workflows.outputPicker.choose")}</strong>
        <span>{value || t("workflows.outputPicker.hint")}</span>
      </span>
      <span className="workflow-output-picker__action">{t(pending ? "workflows.outputPicker.opening" : value ? "workflows.outputPicker.change" : "workflows.outputPicker.browse")}</span>
    </button>
    {error && <p className="workflow-scope-state is-invalid" role="alert">{t(`workflows.outputPicker.${error}`, { directory: matchingAuthority?.layout.exportRoot ?? "" })}</p>}
  </div>;
}
