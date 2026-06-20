import { Maximize, Plus, RotateCcw, ZoomOut } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { GraphColorMode } from "../../types/graph";

interface GraphControlsProps {
  colorMode: GraphColorMode;
  onColorModeChange: (mode: GraphColorMode) => void;
  search: string;
  onSearchChange: (value: string) => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onResetLayout: () => void;
  onRebuild: () => void;
  nodeCount: number;
  edgeCount: number;
}

export function GraphControls({
  colorMode,
  onColorModeChange,
  search,
  onSearchChange,
  onZoomIn,
  onZoomOut,
  onFit,
  onResetLayout,
  onRebuild,
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

      <div className="flex items-center gap-1.5">
        <input
          type="text"
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          placeholder={t("graph.searchPlaceholder")}
          className="h-[28px] w-[200px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-2.5 text-[12px] text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-1 focus:ring-[var(--accent)]"
        />
      </div>

      <div className="ml-auto flex items-center gap-1">
        <span className="mr-2 font-mono text-[11px] text-[var(--text-muted)]">
          {nodeCount} {t("graph.nodesLabel")} · {edgeCount} {t("graph.edgesLabel")}
        </span>
        <IconButton label={t("graph.zoomIn")} onClick={onZoomIn}>
          <Plus size={14} />
        </IconButton>
        <IconButton label={t("graph.zoomOut")} onClick={onZoomOut}>
          <ZoomOut size={14} />
        </IconButton>
        <IconButton label={t("graph.fit")} onClick={onFit}>
          <Maximize size={14} />
        </IconButton>
        <IconButton label={t("graph.resetLayout")} onClick={onResetLayout}>
          <RotateCcw size={14} />
        </IconButton>
        <button
          type="button"
          onClick={onRebuild}
          className="ml-1 h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] px-2.5 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
        >
          {t("graph.rebuild")}
        </button>
      </div>
    </div>
  );
}

interface IconButtonProps {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}

function IconButton({ label, onClick, children }: IconButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      title={label}
      className="flex h-[28px] w-[28px] items-center justify-center rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)] hover:text-[var(--text-primary)]"
    >
      {children}
    </button>
  );
}
