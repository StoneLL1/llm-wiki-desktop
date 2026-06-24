import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Play, X } from "lucide-react";
import type { AgentInfo, AgentKind } from "../../types/agent";
import type { ProviderStatus } from "../../types/llm";
import type { LlmProviderKind } from "../../types/llm";
import { useModalDialog } from "../../hooks/useModalDialog";

export type AgentSkill =
  | "wiki-ingest"
  | "wiki-lint"
  | "wiki-query"
  | "html-beautiful-read"
  | "html-knowledge-card"
  | "html-concept-map"
  | "html-project-report";

export type RunAgentRoute = "auto" | "agent" | "byok";

export interface RunAgentOptions {
  skill: AgentSkill;
  route: RunAgentRoute;
  agent: AgentKind | null;
  provider: LlmProviderKind | null;
  checkpoint: boolean;
  background: boolean;
}

interface ExecChoice {
  key: string;
  label: string;
  hint: string;
  route: RunAgentRoute;
  agent: AgentKind | null;
  provider: LlmProviderKind | null;
}

interface RunAgentDialogProps {
  open: boolean;
  onClose: () => void;
  onRun: (options: RunAgentOptions) => void;
  agents: AgentInfo[];
  providers: ProviderStatus[];
  defaultAgentKind: AgentKind | null;
  /** Pre-selected skill when opened from a specific card (e.g. Ingest). */
  presetSkill?: AgentSkill;
}

const SKILLS: { value: AgentSkill; labelKey: string; group: "core" | "html" }[] = [
  { value: "wiki-ingest", labelKey: "agent.skill.wiki-ingest", group: "core" },
  { value: "wiki-lint", labelKey: "agent.skill.wiki-lint", group: "core" },
  { value: "wiki-query", labelKey: "agent.skill.wiki-query", group: "core" },
  { value: "html-beautiful-read", labelKey: "agent.skill.html-beautiful-read", group: "html" },
  { value: "html-knowledge-card", labelKey: "agent.skill.html-knowledge-card", group: "html" },
  { value: "html-concept-map", labelKey: "agent.skill.html-concept-map", group: "html" },
  { value: "html-project-report", labelKey: "agent.skill.html-project-report", group: "html" },
];

const PROVIDER_LABEL: Record<LlmProviderKind, string> = {
  anthropic: "Anthropic",
  open_ai: "OpenAI",
  google: "Google",
  ollama: "Ollama",
  custom: "Custom",
};

export function RunAgentDialog({
  open,
  onClose,
  onRun,
  agents,
  providers,
  defaultAgentKind,
  presetSkill,
}: RunAgentDialogProps) {
  const { t } = useTranslation();
  const containerRef = useModalDialog({ open, onClose });

  const execChoices = useMemo<ExecChoice[]>(() => {
    const installed = agents.filter((agent) => agent.state === "installed");
    const choices: ExecChoice[] = installed.map((agent, index) => ({
      key: `agent:${agent.kind}`,
      label: agent.command,
      hint: agent.isDefault
        ? t("agent.exec.default")
        : index === 0
          ? t("agent.exec.backup")
          : t("agent.exec.available"),
      route: "agent",
      agent: agent.kind,
      provider: null,
    }));
    for (const provider of providers) {
      if (!provider.config.enabled) continue;
      const hasSecret =
        provider.hasSecret || provider.config.provider === "ollama";
      if (!hasSecret) continue;
      choices.push({
        key: `byok:${provider.config.provider}`,
        label: `BYOK · ${PROVIDER_LABEL[provider.config.provider]}`,
        hint: t("agent.exec.byok"),
        route: "byok",
        agent: null,
        provider: provider.config.provider,
      });
    }
    return choices;
  }, [agents, providers, t]);

  const defaultChoiceKey = useMemo(() => {
    if (defaultAgentKind) {
      const key = `agent:${defaultAgentKind}`;
      if (execChoices.some((choice) => choice.key === key)) return key;
    }
    return execChoices[0]?.key ?? null;
  }, [defaultAgentKind, execChoices]);

  const [skill, setSkill] = useState<AgentSkill>(presetSkill ?? "wiki-ingest");
  const [selectedChoice, setSelectedChoice] = useState<string | null>(null);
  const [checkpoint, setCheckpoint] = useState(true);
  const [background, setBackground] = useState(true);

  useEffect(() => {
    if (open) {
      setSkill(presetSkill ?? "wiki-ingest");
      setSelectedChoice(defaultChoiceKey);
      setCheckpoint(true);
      setBackground(true);
    }
  }, [open, presetSkill, defaultChoiceKey]);

  if (!open) return null;

  const handleSubmit = () => {
    const choice = execChoices.find((entry) => entry.key === selectedChoice);
    if (!choice) return;
    onRun({
      skill,
      route: choice.route,
      agent: choice.agent,
      provider: choice.provider,
      checkpoint,
      background,
    });
  };

  return (
    <div
      ref={containerRef}
      aria-modal="true"
      className="dialog-overlay"
      role="dialog"
      aria-labelledby="run-agent-dialog-title"
      tabIndex={-1}
    >
      <div className="dialog dialog--wide">
        <header className="dialog__head">
          <h2 id="run-agent-dialog-title" className="dialog__title">
            {t("agent.runTitle")}
          </h2>
          <button
            type="button"
            aria-label={t("agent.close")}
            className="btn btn--ghost btn--icon btn--sm"
            onClick={onClose}
            style={{ marginLeft: "auto" }}
          >
            <X size={16} aria-hidden />
          </button>
        </header>

        <div className="dialog__body">
          <div className="formrow">
            <div>
              <div className="formrow__label">{t("agent.run.operation")}</div>
              <div className="formrow__hint">{t("agent.run.operationHint")}</div>
            </div>
            <div className="formrow__control">
              <select
                className="select"
                value={skill}
                onChange={(event) => setSkill(event.target.value as AgentSkill)}
                aria-label={t("agent.run.operation")}
              >
                {SKILLS.map((entry) => (
                  <option key={entry.value} value={entry.value}>
                    {t(entry.labelKey)}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("agent.run.execPath")}</div>
              <div className="formrow__hint">{t("agent.run.execPathHint")}</div>
            </div>
            <div className="formrow__control">
              {execChoices.length === 0 ? (
                <span className="text-[12px]" style={{ color: "var(--text-muted)" }}>
                  {t("agent.run.noExec")}
                </span>
              ) : (
                <div className="seg" role="radiogroup" aria-label={t("agent.run.execPath")}>
                  {execChoices.map((choice) => (
                    <button
                      key={choice.key}
                      type="button"
                      role="radio"
                      aria-checked={selectedChoice === choice.key}
                      className={selectedChoice === choice.key ? "is-active" : undefined}
                      onClick={() => setSelectedChoice(choice.key)}
                      title={choice.hint}
                    >
                      {choice.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("agent.run.gitCheckpoint")}</div>
              <div className="formrow__hint">{t("agent.run.gitCheckpointHint")}</div>
            </div>
            <div className="formrow__control">
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={checkpoint}
                  onChange={(event) => setCheckpoint(event.target.checked)}
                />
                <span>{t("agent.run.gitCheckpointLabel")}</span>
              </label>
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("agent.run.background")}</div>
              <div className="formrow__hint">{t("agent.run.backgroundHint")}</div>
            </div>
            <div className="formrow__control" style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <label className="toggle">
                <input
                  type="checkbox"
                  checked={background}
                  onChange={(event) => setBackground(event.target.checked)}
                  aria-label={t("agent.run.background")}
                />
                <span className="toggle__slider" aria-hidden />
              </label>
              <span className="text-[12px]" style={{ color: "var(--text-muted)" }}>
                {t("agent.run.backgroundLabel")}
              </span>
            </div>
          </div>
        </div>

        <footer className="dialog__foot">
          <button type="button" className="btn" onClick={onClose}>
            {t("agent.run.cancel")}
          </button>
          <button
            type="button"
            className="btn btn--primary"
            onClick={handleSubmit}
            disabled={!selectedChoice}
          >
            <Play size={14} aria-hidden />
            {t("agent.run.run")}
          </button>
        </footer>
      </div>
    </div>
  );
}
