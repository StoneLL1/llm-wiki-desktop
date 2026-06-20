import type { AgentInfo, AgentKind } from "../../types/agent";
import { useTranslation } from "react-i18next";

interface AgentSettingsProps {
  agents: AgentInfo[];
  agentDefault: AgentKind | null;
  onRefresh: () => void;
  onChangeDefault: (agent: AgentKind | null) => void;
}

export function AgentSettings({ agents, agentDefault, onRefresh, onChangeDefault }: AgentSettingsProps) {
  const { t } = useTranslation();

  return (
    <section className="grid gap-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h2 className="m-0 text-[16px] font-semibold">{t("settings.agent.title")}</h2>
          <p className="mt-1 text-[12px] text-[var(--text-muted)]">{t("settings.agent.description")}</p>
        </div>
        <button type="button" className="settings-button settings-button--secondary" onClick={onRefresh}>
          {t("settings.agent.refresh")}
        </button>
      </div>

      <div className="grid gap-2">
        {agents.map((agent) => {
          const selected = agentDefault === agent.kind;
          return (
            <div
              key={agent.kind}
              className="grid min-h-[58px] grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--radius-md)] border border-[var(--border)] px-3 py-2"
            >
              <div className="min-w-0">
                <div className="flex items-center gap-2 text-[13px] font-medium">
                  <span className="font-mono">{agent.command}</span>
                  <span className="rounded-full border border-[var(--border)] px-2 py-0.5 text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
                    {t(`settings.agent.state.${agent.state}`)}
                  </span>
                </div>
                <div className="mt-1 truncate font-mono text-[11px] text-[var(--text-muted)]">
                  {agent.version ?? agent.executablePath ?? agent.error ?? t("agent.notFound")}
                </div>
              </div>
              <div className="flex items-center gap-2">
                {selected ? (
                  <button type="button" className="settings-button settings-button--secondary" onClick={() => onChangeDefault(null)}>
                    {t("settings.agent.clear")}
                  </button>
                ) : null}
                <button
                  type="button"
                  disabled={agent.state !== "installed"}
                  className="settings-button"
                  onClick={() => onChangeDefault(agent.kind)}
                >
                  {selected ? t("settings.agent.selected") : t("settings.agent.makeDefault")}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </section>
  );
}
