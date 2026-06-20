import { Bot, CircleAlert, CircleCheck, Play, RefreshCw, Terminal, XCircle } from "lucide-react";
import type { AgentInfo } from "../../types/agent";
import type { AgentKind } from "../../types/agent";
import type { BackendTask } from "../../types/task";
import { useTranslation } from "react-i18next";

interface AgentViewProps {
  agents: AgentInfo[];
  providerCount: number;
  onDetect: () => void;
  onCompile: () => void;
  onSetDefault?: (agent: AgentKind) => void;
  tasks?: BackendTask[];
  onOpenTask?: (taskId: string) => void;
}

const stateIcon = {
  installed: <CircleCheck aria-hidden size={16} className="text-[var(--accent)]" />,
  missing: <XCircle aria-hidden size={16} className="text-[var(--text-muted)]" />,
  failed: <CircleAlert aria-hidden size={16} className="text-[var(--danger)]" />,
};

export function AgentView({ agents, providerCount, onDetect, onCompile, onSetDefault, tasks = [], onOpenTask }: AgentViewProps) {
  const { t } = useTranslation();
  return (
    <div className="agent-grid">
      <section className="panel">
        <div className="panel-header flex items-center">
          <span>{t("agent.detected")}</span>
          <button type="button" className="ml-auto inline-flex h-[28px] items-center gap-1 rounded-[var(--radius-sm)] border border-[var(--border)] px-2 text-[12px]" onClick={onDetect}>
            <RefreshCw size={14} /> {t("agent.detect")}
          </button>
        </div>
        <div className="mt-3 flex flex-col gap-2">
          {agents.map((agent) => (
            <div key={agent.kind} className="grid min-h-[58px] grid-cols-[32px_minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border-subtle)] px-3 py-2">
              <div className="grid h-8 w-8 place-items-center rounded-[var(--radius-sm)] bg-[var(--surface-muted)]"><Bot size={16} /></div>
              <div className="min-w-0">
                <div className="flex items-center gap-2 text-[13px] font-semibold">
                  <span className="font-mono">{agent.command}</span>
                  {agent.isDefault ? <span className="badge badge--accent">{t("agent.default")}</span> : null}
                  {stateIcon[agent.state]}
                </div>
                <div className="mt-1 truncate font-mono text-[11px] text-[var(--text-muted)]">
                  {agent.version ?? agent.error ?? t("agent.notFound")}
                </div>
                {agent.state === "missing" ? <div className="mt-1 font-mono text-[11px] text-[var(--text-secondary)]">{agent.installGuidance}</div> : null}
              </div>
              {agent.state === "installed" && !agent.isDefault ? (
                <button type="button" className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] px-2 text-[11px]" onClick={() => onSetDefault?.(agent.kind)}>{t("agent.setDefault")}</button>
              ) : <span className="text-[11px] uppercase tracking-[0.08em] text-[var(--text-muted)]">{agent.state}</span>}
            </div>
          ))}
        </div>
      </section>

      <section className="grid grid-cols-[minmax(0,1fr)_240px] gap-3">
        <div className="panel">
          <div className="panel-header"><Terminal size={14} /> {t("agent.coreOperation")}</div>
          <p className="my-3 text-[12px] text-[var(--text-secondary)]">{t("agent.compileDescription")}</p>
          <button type="button" onClick={onCompile} className="inline-flex h-[30px] items-center gap-2 rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] text-[var(--text-inverse)]">
            <Play size={14} /> {t("agent.compile")}
          </button>
        </div>
        <div className="panel">
          <div className="panel-header">{t("agent.byokFallback")}</div>
          <div className="mt-3 text-[22px] font-semibold">{providerCount}</div>
          <div className="text-[11px] text-[var(--text-muted)]">{t("agent.providersConfigured")}</div>
        </div>
      </section>
      <section className="panel">
        <div className="panel-header">{t("agent.tasksAndLogs")}</div>
        <div className="mt-3 flex flex-col gap-2">
          {tasks.length === 0 ? <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("agent.noTasks")}</p> : tasks.map((task) => (
            <button key={task.id} type="button" onClick={() => onOpenTask?.(task.id)} className="grid min-h-[42px] grid-cols-[1fr_auto] items-center rounded-[var(--radius-sm)] border border-[var(--border-subtle)] px-3 text-left">
              <span><span className="block text-[12px] font-medium">{task.title}</span><span className="font-mono text-[11px] text-[var(--text-muted)]">{task.taskType}</span></span>
              <span className="text-[11px] text-[var(--text-muted)]">{task.status}</span>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
