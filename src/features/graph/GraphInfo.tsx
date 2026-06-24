import { useTranslation } from "react-i18next";

import type { GraphNode } from "../../types/graph";

interface GraphInfoProps {
  /** Display zoom ratio (1 = fit). `null` until the first camera event. */
  zoom: number | null;
  selectedNode: GraphNode | null;
}

/**
 * Top-right floating mono info card (UI-Frontend-design/assets/app.css
 * `.graph-info`): live camera zoom, the selected node label, and its degree.
 * `zoom` is fed from a camera state subscription in `GraphView`; the selected
 * node is read from the store-backed data so it updates on click.
 */
export function GraphInfo({ zoom, selectedNode }: GraphInfoProps) {
  const { t } = useTranslation();
  return (
    <div className="graph-info" role="status" aria-live="polite">
      <div className="graph-info__row">
        <span className="graph-info__label">{t("graph.info.zoom")}</span>
        <span className="graph-info__value">{zoom != null ? `${zoom.toFixed(1)}×` : "—"}</span>
      </div>
      <div className="graph-info__row">
        <span className="graph-info__label">{t("graph.info.selected")}</span>
        <span className="graph-info__value truncate max-w-[180px]">
          {selectedNode ? selectedNode.label : t("graph.info.none")}
        </span>
      </div>
      <div className="graph-info__row">
        <span className="graph-info__label">{t("graph.info.degree")}</span>
        <span className="graph-info__value">{selectedNode ? selectedNode.degree : t("graph.info.none")}</span>
      </div>
    </div>
  );
}
