import { useTranslation } from "react-i18next";

import type { GraphColorMode, GraphData } from "../../types/graph";
import { PAGE_TYPE_LABEL_KEYS, type WikiPageType } from "../../types/wiki";
import { legendEntries } from "./legendEntries";

interface GraphLegendProps {
  data: GraphData;
  colorMode: GraphColorMode;
  hiddenTypes: Set<WikiPageType>;
  degreeThreshold: number;
}

/**
 * Bottom-left floating legend (UI-Frontend-design/assets/app.css
 * `.graph-legend`). Content is dynamic per color mode: type swatches with
 * counts, the top communities + "other", or a plain label. Filtered-out types
 * are dimmed and counted as zero so the legend tracks the on-canvas state.
 */
export function GraphLegend({ data, colorMode, hiddenTypes, degreeThreshold }: GraphLegendProps) {
  const { t } = useTranslation();
  const entries = legendEntries(colorMode, data, resolveLabels(t), hiddenTypes, degreeThreshold);

  if (colorMode === "plain") {
    return (
      <div className="graph-legend" role="status" aria-label={t("graph.legend.title.plain")}>
        <div className="graph-legend__title">{t("graph.legend.title.plain")}</div>
        <div className="graph-legend__row">
          <span className="swatch" style={{ background: "#9b9b9b" }} aria-hidden />
          {data.nodes.length} {t("graph.nodesLabel")}
        </div>
      </div>
    );
  }

  if (entries.length === 0) return null;

  return (
    <div
      className="graph-legend"
      role="status"
      aria-label={colorMode === "community" ? t("graph.legend.title.community") : t("graph.legend.title.type")}
    >
      <div className="graph-legend__title">
        {colorMode === "community" ? t("graph.legend.title.community") : t("graph.legend.title.type")}
      </div>
      {entries.map((entry) => {
        const dim = entry.count === 0;
        return (
          <div key={entry.key} className={`graph-legend__row${dim ? " is-dim" : ""}`}>
            <span className="swatch" style={{ background: entry.color }} aria-hidden />
            <span>{entry.label}</span>
            <span className="graph-legend__count">{dim ? t("graph.legend.hidden") : entry.count}</span>
          </div>
        );
      })}
    </div>
  );
}

function resolveLabels(t: (key: string) => string): Record<WikiPageType, string> {
  const labels = {} as Record<WikiPageType, string>;
  (Object.keys(PAGE_TYPE_LABEL_KEYS) as WikiPageType[]).forEach((type) => {
    labels[type] = t(PAGE_TYPE_LABEL_KEYS[type]);
  });
  return labels;
}
