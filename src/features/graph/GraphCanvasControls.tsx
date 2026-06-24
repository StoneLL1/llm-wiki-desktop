import { Maximize, Plus, RotateCcw, ZoomOut } from "lucide-react";
import { useTranslation } from "react-i18next";

interface GraphCanvasControlsProps {
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  onResetLayout: () => void;
}

/**
 * Floating vertical zoom/layout controls pinned to the top-left of the graph
 * canvas (UI-Frontend-design/assets/app.css `.graph-controls`: a column of
 * 30px icon buttons). The top toolbar keeps the color-mode segment, search,
 * rebuild and counts; these viewport actions live on the canvas so they are
 * next to what they affect.
 */
export function GraphCanvasControls({ onZoomIn, onZoomOut, onFit, onResetLayout }: GraphCanvasControlsProps) {
  const { t } = useTranslation();
  return (
    <div className="graph-float-controls" role="toolbar" aria-label={t("graph.controls.toolbar")}>
      <button type="button" aria-label={t("graph.zoomIn")} title={t("graph.zoomIn")} onClick={onZoomIn}>
        <Plus size={16} />
      </button>
      <button type="button" aria-label={t("graph.zoomOut")} title={t("graph.zoomOut")} onClick={onZoomOut}>
        <ZoomOut size={16} />
      </button>
      <button type="button" aria-label={t("graph.fit")} title={t("graph.fit")} onClick={onFit}>
        <Maximize size={16} />
      </button>
      <button type="button" aria-label={t("graph.resetLayout")} title={t("graph.resetLayout")} onClick={onResetLayout}>
        <RotateCcw size={16} />
      </button>
    </div>
  );
}
