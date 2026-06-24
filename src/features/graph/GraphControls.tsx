import { Download, RotateCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { GraphColorMode } from "../../types/graph";

interface GraphControlsProps {
  colorMode: GraphColorMode;
  onColorModeChange: (mode: GraphColorMode) => void;
  search: string;
  onSearchChange: (value: string) => void;
  onRebuild: () => void;
  onExportSvg: () => void;
  nodeCount: number;
  edgeCount: number;
}

/**
 * Top toolbar for the graph view (UI-Frontend-design/graph.html `main__toolbar`):
 * color-mode segment, node search, rebuild, SVG export and a node/edge count.
 * Viewport zoom/fit/reset actions are split off into the floating
 * `GraphCanvasControls` pinned on the canvas itself.
 */
export function GraphControls({
  colorMode,
  onColorModeChange,
  search,
  onSearchChange,
  onRebuild,
  onExportSvg,
  nodeCount,
  edgeCount,
}: GraphControlsProps) {
  const { t } = useTranslation();
  const modes: GraphColorMode[] = ["type", "community", "plain"];
  return (
    <div className="flex h-[44px] items-center gap-2 border-b border-[var(--border)] bg-[var(--surface)] px-3">
      <div className="seg flex items-center rounded-[var(--radius-md)] border border-[var(--border)] p-0.5">
        {modes.map((mode) => (
          <button
            key={mode}
            type="button"
            onClick={() => onColorModeChange(mode)}
            className={`h-[28px] rounded-[calc(var(--radius-md)-2px)] px-2.5 text-[12px] font-medium transition-colors ${
              colorMode === mode
                ? "bg-[var(--foreground)] text-[var(--text-inverse)]"
                : "text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
            }`}
          >
            {t(`graph.colorMode.${mode}`)}
          </button>
        ))}
      </div>

      <input
        type="text"
        value={search}
        onChange={(event) => onSearchChange(event.target.value)}
        placeholder={t("graph.searchPlaceholder")}
        className="h-[28px] w-[200px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-2.5 text-[12px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
      />

      <div className="ml-auto flex items-center gap-2">
        <span className="mr-1 font-mono text-[11px] text-[var(--text-muted)]">
          {nodeCount} {t("graph.nodesLabel")} · {edgeCount} {t("graph.edgesLabel")}
        </span>
        <button
          type="button"
          onClick={onRebuild}
          className="flex h-[28px] items-center gap-1.5 rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
        >
          <RotateCw size={13} />
          {t("graph.rebuild")}
        </button>
        <button
          type="button"
          onClick={onExportSvg}
          aria-label={t("graph.exportSvg")}
          title={t("graph.exportSvg")}
          className="flex h-[28px] items-center gap-1.5 rounded-[var(--radius-md)] bg-[var(--accent)] px-2.5 text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[var(--accent-hover)]"
        >
          <Download size={13} />
          {t("graph.exportSvg")}
        </button>
      </div>
    </div>
  );
}
