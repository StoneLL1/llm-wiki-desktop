import {
  CheckCircle2,
  ListChecks,
  Plus,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Square,
  Upload,
  AlertTriangle,
} from "lucide-react";
import type { AgentDetectionState, AgentInfo, AgentKind } from "../../types/agent";
import type { ProviderStatus } from "../../types/llm";
import type { BackendTask, TaskStatus } from "../../types/task";
import { useTranslation } from "react-i18next";
import { isTerminalStatus } from "../../types/task";
import type { AgentSkill } from "./RunAgentDialog";

interface AgentViewProps {
  agents: AgentInfo[];
  providers: ProviderStatus[];
  tasks: BackendTask[];
  onDetect: () => void;
  onRunAgent: (presetSkill?: AgentSkill) => void;
  onSetDefault?: (agent: AgentKind) => void;
  onCancelTask?: (taskId: string) => void;
  onOpenTask?: (taskId: string) => void;
  onNavigate?: (view: "lint" | "chat" | "exports") => void;
}

const AGENT_INITIALS: Record<AgentKind, string> = {
  claude: "CL",
  codex: "CX",
  openclaw: "OC",
  hermes: "HE",
};

const AGENT_VENDOR: Record<AgentKind, string> = {
  claude: "Anthropic · claude-cli",
  codex: "OpenAI · codex-cli",
  openclaw: "openclaw",
  hermes: "hermes",
};

const PROVIDER_DISPLAY_ORDER: { kind: ProviderStatus["config"]["provider"]; label: string; hint: string }[] = [
  { kind: "anthropic", label: "Anthropic", hint: "claude-sonnet-4-6" },
  { kind: "open_ai", label: "OpenAI", hint: "gpt-4o" },
  { kind: "google", label: "Google", hint: "Gemini Pro / Flash" },
  { kind: "ollama", label: "Ollama", hint: "localhost:11434" },
];

function stateDotClass(state: AgentDetectionState): string {
  if (state === "installed") return "dotstatus dotstatus--ok";
  return "dotstatus dotstatus--err";
}

function taskIconClass(status: TaskStatus): string {
  if (status === "failed") return "task-row__icon is-err";
  if (isTerminalStatus(status)) return "task-row__icon is-ok";
  return "task-row__icon is-busy";
}

function progressPercent(task: BackendTask): number | null {
  if (!task.progress) return null;
  const { current, total } = task.progress;
  if (!total || total <= 0) return null;
  return Math.min(100, Math.max(0, Math.round((current / total) * 100)));
}

export function AgentView({
  agents,
  providers,
  tasks,
  onDetect,
  onRunAgent,
  onSetDefault,
  onCancelTask,
  onOpenTask,
  onNavigate,
}: AgentViewProps) {
  const { t } = useTranslation();

  const runningCount = tasks.filter(
    (task) => !isTerminalStatus(task.status) && task.status !== "waiting_for_confirmation",
  ).length;
  const doneToday = tasks.filter((task) => task.status === "succeeded").length;

  return (
    <div className="agent-grid">
      {/* Detected Agent CLIs */}
      <section className="agent-section panel">
        <div className="panel-header" style={{ marginBottom: "var(--sp-3)" }}>
          <span>{t("agent.detected")}</span>
          <button
            type="button"
            className="btn btn--sm"
            style={{ marginLeft: "auto" }}
            onClick={onDetect}
          >
            <RefreshCw size={12} aria-hidden />
            {t("agent.detect")}
          </button>
        </div>
        <div className="agent-cli-list">
          {agents.length === 0 ? (
            <p className="text-[12px]" style={{ color: "var(--text-muted)", margin: 0 }}>
              {t("agent.noTasks")}
            </p>
          ) : (
            agents.map((agent) => {
              const isMissing = agent.state !== "installed";
              const rowClass =
                "cli-row" +
                (agent.isDefault ? " is-default" : "") +
                (isMissing ? " is-missing" : "");
              return (
                <div key={agent.kind} className={rowClass}>
                  <div className="cli-row__icon" aria-hidden>
                    {AGENT_INITIALS[agent.kind]}
                  </div>
                  <div className="min-w-0">
                    <div className="cli-row__name">
                      <span className="font-mono">{agent.command}</span>
                      <span className="cli-row__vendor">{AGENT_VENDOR[agent.kind]}</span>
                    </div>
                    <div className="cli-row__path">
                      {agent.state === "installed"
                        ? `${agent.executablePath ?? agent.command} · v${agent.version ?? "?"} · ${t("agent.cli.signed")}`
                        : agent.error ?? t("agent.detectedMissing")}
                    </div>
                    {isMissing && agent.installGuidance ? (
                      <div
                        className="font-mono"
                        style={{
                          fontSize: "11px",
                          color: "var(--text-secondary)",
                          marginTop: 4,
                        }}
                      >
                        {agent.installGuidance}
                      </div>
                    ) : null}
                  </div>
                  {agent.isDefault ? (
                    <span className="badge badge--accent">{t("agent.default")}</span>
                  ) : agent.state === "installed" ? (
                    <span className="badge">{t("agent.exec.backup")}</span>
                  ) : (
                    <span className="badge badge--outline">{t("agent.cli.state.missing")}</span>
                  )}
                  <span className={stateDotClass(agent.state)} aria-hidden />
                  {agent.state === "installed" && !agent.isDefault ? (
                    <button
                      type="button"
                      className="btn btn--sm btn--ghost"
                      onClick={() => onSetDefault?.(agent.kind)}
                    >
                      {t("agent.setDefault")}
                    </button>
                  ) : (
                    <span style={{ width: 1 }} aria-hidden />
                  )}
                </div>
              );
            })
          )}
        </div>
      </section>

      {/* BYOK fallback */}
      <section className="agent-section">
        <h2 className="agent-section__title">
          <span>{t("agent.byokFallback")}</span>
          <span className="agent-section__meta">{t("agent.byokMeta")}</span>
        </h2>
        <div className="summarygrid">
          {PROVIDER_DISPLAY_ORDER.map((entry) => {
            const status = providers.find((p) => p.config.provider === entry.kind);
            const enabled = status?.config.enabled ?? false;
            const hasSecret = status?.hasSecret ?? false;
            const isConfigured = enabled && (hasSecret || entry.kind === "ollama");
            const valueKey = isConfigured
              ? entry.kind === "ollama"
                ? "agent.byok.local"
                : "agent.byok.configured"
              : "agent.byok.unconfigured";
            const valueClass = isConfigured
              ? "sumcard__value sumcard__value--configured"
              : "sumcard__value sumcard__value--unconfigured";
            let hint = isConfigured
              ? status?.config.model || entry.hint
              : t("agent.byok.gotoSettings");
            if (entry.kind === "ollama" && isConfigured && !status?.config.model) {
              hint = `${entry.hint} · ${t("agent.byok.noModels")}`;
            }
            return (
              <div key={entry.kind} className="sumcard sumcard--provider">
                <div className="sumcard__label">{entry.label}</div>
                <div className={valueClass} style={{ fontSize: 14, fontFamily: "var(--font-ui)", fontWeight: 600 }}>
                  {t(valueKey)}
                </div>
                <div className="sumcard__hint">{hint}</div>
              </div>
            );
          })}
        </div>
      </section>

      {/* Core operations four-grid */}
      <section className="agent-section">
        <h2 className="agent-section__title">
          <span>{t("agent.coreOperation")}</span>
        </h2>
        <div className="ingest-grid">
          <button
            type="button"
            className="ingest-card is-primary"
            onClick={() => onRunAgent("wiki-ingest")}
          >
            <div className="ingest-card__icon" aria-hidden>
              <Upload size={16} />
            </div>
            <div className="ingest-card__title">{t("agent.ingest.primary.title")}</div>
            <div className="ingest-card__desc">{t("agent.ingest.primary.desc")}</div>
            <div className="ingest-card__cta">{t("agent.ingest.primary.cta")}</div>
          </button>
          <button
            type="button"
            className="ingest-card"
            onClick={() => onNavigate?.("lint")}
          >
            <div className="ingest-card__icon" aria-hidden>
              <ShieldCheck size={16} />
            </div>
            <div className="ingest-card__title">{t("agent.ingest.lint.title")}</div>
            <div className="ingest-card__desc">{t("agent.ingest.lint.desc")}</div>
            <div className="ingest-card__cta">{t("agent.ingest.lint.cta")}</div>
          </button>
          <button
            type="button"
            className="ingest-card"
            onClick={() => onNavigate?.("chat")}
          >
            <div className="ingest-card__icon" aria-hidden>
              <Sparkles size={16} />
            </div>
            <div className="ingest-card__title">{t("agent.ingest.query.title")}</div>
            <div className="ingest-card__desc">{t("agent.ingest.query.desc")}</div>
            <div className="ingest-card__cta">{t("agent.ingest.query.cta")}</div>
          </button>
          <button
            type="button"
            className="ingest-card"
            onClick={() => onNavigate?.("exports")}
          >
            <div className="ingest-card__icon" aria-hidden>
              <Plus size={16} />
            </div>
            <div className="ingest-card__title">{t("agent.ingest.html.title")}</div>
            <div className="ingest-card__desc">{t("agent.ingest.html.desc")}</div>
            <div className="ingest-card__cta">{t("agent.ingest.html.cta")}</div>
          </button>
        </div>
      </section>

      {/* Tasks */}
      <section className="agent-section">
        <h2 className="agent-section__title">
          <span>{t("agent.tasksAndLogs")}</span>
          <span className="agent-section__meta">
            {t("agent.tasksMeta", { running: runningCount, done: doneToday })}
          </span>
        </h2>
        <div className="task-list">
          {tasks.length === 0 ? (
            <p className="text-[12px]" style={{ color: "var(--text-muted)", margin: 0 }}>
              {t("agent.noTasks")}
            </p>
          ) : (
            tasks.map((task) => {
              const percent = progressPercent(task);
              const terminal = isTerminalStatus(task.status);
              const cancellable = task.cancellable && !terminal;
              const rowClass = "task-row" + (terminal ? " is-terminal" : "");
              return (
                <div key={task.id} className={rowClass}>
                  <div className={taskIconClass(task.status)} aria-hidden>
                    {task.status === "failed" ? (
                      <AlertTriangle size={16} />
                    ) : terminal ? (
                      <CheckCircle2 size={16} />
                    ) : (
                      <RefreshCw size={14} className="animate-spin" />
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => onOpenTask?.(task.id)}
                    style={{
                      all: "unset",
                      cursor: "pointer",
                      display: "block",
                      minWidth: 0,
                      flex: "1 1 auto",
                    }}
                    title={t("agent.task.viewLogs")}
                  >
                    <div className="task-row__title">{task.title}</div>
                    <div className="task-row__sub">
                      {task.taskType} · {task.status}
                    </div>
                    {percent !== null ? (
                      <div className="task-row__progress">
                        <div
                          className="progress progress--sm"
                          role="progressbar"
                          aria-valuenow={percent}
                          aria-valuemin={0}
                          aria-valuemax={100}
                          aria-label={task.title}
                        >
                          <div className="progress__bar">
                            <div
                              className="progress__fill"
                              style={{ width: `${percent}%` }}
                            />
                          </div>
                          <span className="progress__num">{percent}%</span>
                        </div>
                      </div>
                    ) : null}
                  </button>
                  <div className="task-row__actions">
                    {cancellable ? (
                      <button
                        type="button"
                        className="btn btn--sm"
                        onClick={() => onCancelTask?.(task.id)}
                      >
                        <Square size={12} aria-hidden />
                        {t("agent.task.cancel")}
                      </button>
                    ) : (
                      <button
                        type="button"
                        className="btn btn--sm btn--ghost"
                        onClick={() => onOpenTask?.(task.id)}
                        aria-label={t("agent.task.viewLogs")}
                      >
                        <ListChecks size={14} aria-hidden />
                      </button>
                    )}
                  </div>
                </div>
              );
            })
          )}
        </div>
      </section>
    </div>
  );
}
