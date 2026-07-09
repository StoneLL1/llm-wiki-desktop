import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  MessageCircle,
  Check,
  Settings as SettingsIcon,
  ShieldCheck,
  Upload,
} from "lucide-react";
import { useProjectStore } from "../../stores/projectStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useNavigationStore } from "../../stores/navigationStore";
import type { AgentInfo } from "../../types/agent";
import { RightPanelHeader } from "../../components/app/RightPanelHeader";

interface AgentRightPanelProps {
  agents: AgentInfo[];
  onRunIngest?: () => void;
}

// Skill templates the app knows about. The backend does not yet expose a
// list_skills / check_skill_templates command, so availability is shown as
// "known" rather than verified-on-disk; html-project-report is flagged with a
// warning to mirror the design's "missing template" affordance.
const KNOWN_SKILLS: { id: string; warn?: boolean }[] = [
  { id: "wiki-ingest" },
  { id: "wiki-lint" },
  { id: "wiki-query" },
  { id: "html-beautiful-read" },
  { id: "html-knowledge-card" },
  { id: "html-project-report", warn: true },
  { id: "html-concept-map" },
];

const CONTEXT_WINDOW_MIN = 4_000;
const CONTEXT_WINDOW_MAX = 1_000_000;
const CONTEXT_WINDOW_STEP = 1_000;

function formatContextWindow(value: number): string {
  if (value >= 1_000_000) return "1M";
  if (value >= 1_000) return `${Math.round(value / 1_000)}K`;
  return String(value);
}

export function AgentRightPanel({ agents, onRunIngest }: AgentRightPanelProps) {
  const { t } = useTranslation();
  const project = useProjectStore((state) => state.currentProject);
  const settings = useSettingsStore((state) => state.settings);
  const loadSettings = useSettingsStore((state) => state.loadSettings);
  const persistPatch = useSettingsStore((state) => state.persistPatch);
  const setActiveView = useNavigationStore((state) => state.setActiveView);
  const openSettings = useNavigationStore((state) => state.openSettings);

  useEffect(() => {
    if (project.projectId) {
      void loadSettings(project.projectId, project.rootPath);
    }
  }, [project.projectId, project.rootPath, loadSettings]);

  const defaultAgent = agents.find((agent) => agent.isDefault) ?? null;

  const handleContextWindow = (value: number) => {
    void persistPatch(project.projectId, project.rootPath, {
      contextWindow: value,
    });
  };

  return (
    <aside
      id="right-context-panel"
      aria-label={t("agent.rightPanel.title")}
      className="right-panel"
    >
      <RightPanelHeader title={t("agent.rightPanel.title")} />

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {/* Default Agent meta */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
            {t("agent.rightPanel.defaultAgent")}
          </h4>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-[12px]">
            <dt className="font-medium text-[var(--text-muted)]">{t("agent.rightPanel.current")}</dt>
            <dd className="m-0 font-mono text-[11.5px]" style={{ color: defaultAgent ? "var(--accent-hover)" : "var(--text-muted)" }}>
              {defaultAgent ? `${defaultAgent.command} · v${defaultAgent.version ?? "?"}` : t("agent.rightPanel.none")}
            </dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("agent.rightPanel.path")}</dt>
            <dd className="m-0 truncate font-mono text-[11.5px] text-[var(--text-muted)]">
              {defaultAgent?.executablePath ?? "—"}
            </dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("agent.rightPanel.background")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">
              {settings.closeBehavior === "minimize_to_tray"
                ? t("agent.rightPanel.enabled")
                : t("agent.rightPanel.disabled")}
            </dd>
            <dt className="font-medium text-[var(--text-muted)]">{t("agent.rightPanel.notification")}</dt>
            <dd className="m-0 font-mono text-[11.5px]">{t("agent.rightPanel.systemNotify")}</dd>
          </dl>
        </div>

        {/* Skill system checklist */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
            {t("agent.rightPanel.skillSystem")}
          </h4>
          <ul className="m-0 flex flex-col gap-1.5 p-0" style={{ listStyle: "none" }}>
            {KNOWN_SKILLS.map((skill) => (
              <li
                key={skill.id}
                className="flex items-center gap-2 text-[12px]"
                title={skill.warn ? t("agent.rightPanel.skillMissing") : undefined}
              >
                {skill.warn ? (
                  <AlertTriangle size={12} style={{ color: "var(--warning)" }} aria-hidden />
                ) : (
                  <Check size={12} style={{ color: "var(--accent)" }} aria-hidden />
                )}
                <span className="font-mono">{skill.id}</span>
                {skill.warn ? (
                  <span
                    className="ml-auto text-[10.5px]"
                    style={{ color: "var(--text-muted)" }}
                  >
                    {t("agent.rightPanel.missingTemplate")}
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        </div>

        {/* Context window slider */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
            {t("agent.rightPanel.contextWindow")}
          </h4>
          <div className="flex flex-col gap-1.5">
            <div className="flex justify-between text-[12px]">
              <span>{t("agent.rightPanel.currentLabel")}</span>
              <span className="font-mono" style={{ color: "var(--accent-hover)" }}>
                {formatContextWindow(settings.contextWindow)}
              </span>
            </div>
            <input
              type="range"
              min={CONTEXT_WINDOW_MIN}
              max={CONTEXT_WINDOW_MAX}
              step={CONTEXT_WINDOW_STEP}
              value={settings.contextWindow}
              onChange={(event) => handleContextWindow(Number(event.target.value))}
              aria-label={t("agent.rightPanel.contextWindow")}
            />
            <div className="flex justify-between font-mono text-[10.5px] text-[var(--text-muted)]">
              <span>4K</span>
              <span>32K</span>
              <span>128K</span>
              <span>512K</span>
              <span>1M</span>
            </div>
            <p className="m-0 mt-1.5 text-[11px] text-[var(--text-muted)]">
              {t("agent.rightPanel.contextHint")}
            </p>
          </div>
        </div>

        {/* Safety boundaries (invariants — see CLAUDE.md hard rules) */}
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h4 className="mb-2 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
            <ShieldCheck size={12} aria-hidden />
            {t("agent.rightPanel.safetyBoundary")}
          </h4>
          <div className="flex flex-col gap-1.5 text-[12px]">
            <label className="checkbox" title={t("agent.rightPanel.safety.invariant")}>
              <input type="checkbox" checked disabled readOnly />
              <span>{t("agent.rightPanel.safety.checkpoint")}</span>
            </label>
            <label className="checkbox" title={t("agent.rightPanel.safety.invariant")}>
              <input type="checkbox" checked disabled readOnly />
              <span>{t("agent.rightPanel.safety.highRisk")}</span>
            </label>
            <label className="checkbox" title={t("agent.rightPanel.safety.invariant")}>
              <input type="checkbox" checked disabled readOnly />
              <span>{t("agent.rightPanel.safety.conflictCheck")}</span>
            </label>
            <label className="checkbox" title={t("agent.rightPanel.safety.invariant")}>
              <input type="checkbox" disabled readOnly />
              <span>{t("agent.rightPanel.safety.allowInstall")}</span>
            </label>
          </div>
        </div>

        {/* Quick actions */}
        <div className="py-3">
          <h4 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--text-muted)]">
            {t("agent.rightPanel.quickActions")}
          </h4>
          <div className="flex flex-col gap-1.5">
            <button
              type="button"
              className="btn btn--sm"
              style={{ justifyContent: "flex-start" }}
              onClick={() => onRunIngest?.()}
            >
              <Upload size={12} aria-hidden />
              {t("agent.rightPanel.action.ingest")}
            </button>
            <button
              type="button"
              className="btn btn--sm"
              style={{ justifyContent: "flex-start" }}
              onClick={() => setActiveView("lint")}
            >
              <ShieldCheck size={12} aria-hidden />
              {t("agent.rightPanel.action.lint")}
            </button>
            <button
              type="button"
              className="btn btn--sm"
              style={{ justifyContent: "flex-start" }}
              onClick={() => setActiveView("chat")}
            >
              <MessageCircle size={12} aria-hidden />
              {t("agent.rightPanel.action.query")}
            </button>
            <button
              type="button"
              className="btn btn--sm"
              style={{ justifyContent: "flex-start" }}
              onClick={() => openSettings()}
            >
              <SettingsIcon size={12} aria-hidden />
              {t("agent.rightPanel.action.settings")}
            </button>
          </div>
        </div>
      </div>
    </aside>
  );
}
