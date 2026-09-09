import { FileText, Search, type LucideIcon } from "lucide-react";
import { useId, type InputHTMLAttributes } from "react";
import type { WorkflowRouteSelection } from "../../types/workflow";

/** Native radios retain arrow-key navigation and fieldset disabled behavior. */
export function WorkflowChoice({ name, label, description, icon: Icon, ...input }: {
  name: string;
  label: string;
  description?: string;
  icon?: LucideIcon;
} & Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "name">) {
  const descriptionId = useId();
  return <label className="workflow-choice">
    <input {...input} type="radio" name={name} aria-label={label} aria-describedby={description ? descriptionId : undefined} />
    {Icon && <Icon size={18} aria-hidden="true" />}
    <span><strong>{label}</strong>{description && <small id={descriptionId}>{description}</small>}</span>
  </label>;
}

export function WorkflowSearchField(props: InputHTMLAttributes<HTMLInputElement>) {
  return <div className="workflow-search-field"><Search size={15} aria-hidden="true" /><input {...props} type="search" /></div>;
}

export function WorkflowPageLabel({ path }: { path: string }) {
  const name = path.split(/[\\/]/).pop()?.replace(/\.md$/i, "") || path;
  return <><FileText size={15} aria-hidden="true" /><span className="workflow-page-label" title={path}><span>{name}</span><code>{path}</code></span></>;
}

export function workflowRouteLabel(route: WorkflowRouteSelection, t: (key: string) => string): string {
  const names: Record<string, string> = { codex: "Codex", claude: "Claude Code", claude_code: "Claude Code", openclaw: "OpenClaw", hermes: "Hermes", openai: "OpenAI", anthropic: "Anthropic" };
  const id = route.kind === "agent" ? route.agent : route.provider;
  return `${names[id] ?? id} · ${t(route.kind === "agent" ? "workflows.route.agent" : "workflows.route.byok")}`;
}
