import { ExternalLink, Star } from "lucide-react";
import { useTranslation } from "react-i18next";

import { PAGE_TYPE_COLORS, type GraphNode } from "../../types/graph";
import { PAGE_TYPE_LABEL_KEYS } from "../../types/wiki";

interface GraphInspectorProps {
  node: GraphNode | null;
  neighborCount: number;
  onOpenPage: () => void;
}

export function GraphInspector({ node, neighborCount, onOpenPage }: GraphInspectorProps) {
  const { t } = useTranslation();
  if (!node) {
    return (
      <p className="px-4 py-3 text-[12px] leading-5 text-[var(--text-muted)]">
        {t("graph.inspector.empty")}
      </p>
    );
  }

  return (
    <div className="px-4 py-3">
      <div className="border-b border-[var(--border-subtle)] py-3">
        <div className="mb-1 flex items-center gap-2">
          <span
            className="inline-block h-[10px] w-[10px] rounded-full"
            style={{
              background: PAGE_TYPE_COLORS[node.type as keyof typeof PAGE_TYPE_COLORS] ?? "#9b9b9b",
            }}
            aria-hidden
          />
          <h4 className="m-0 text-[13px] font-semibold text-[var(--text-primary)]">{node.label}</h4>
          {node.starred ? <Star size={12} className="text-[var(--accent)]" /> : null}
        </div>
        <p className="m-0 break-all font-mono text-[11px] text-[var(--text-muted)]">{node.path}</p>
      </div>

      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 border-b border-[var(--border-subtle)] py-3 text-[12px]">
        <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.type")}</dt>
        <dd className="m-0 text-[var(--text-primary)]">{t(PAGE_TYPE_LABEL_KEYS[node.type])}</dd>
        <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.degree")}</dt>
        <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{node.degree}</dd>
        <dt className="font-medium text-[var(--text-muted)]">{t("graph.inspector.neighbors")}</dt>
        <dd className="m-0 font-mono text-[11.5px] text-[var(--text-primary)]">{neighborCount}</dd>
      </dl>

      {node.tags.length > 0 ? (
        <div className="border-b border-[var(--border-subtle)] py-3">
          <h5 className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("graph.inspector.tags")}
          </h5>
          <div className="flex flex-wrap gap-1.5">
            {node.tags.map((tag) => (
              <span
                key={tag}
                className="rounded-[var(--radius-pill)] bg-[var(--surface-muted)] px-2 py-0.5 text-[11px] text-[var(--text-secondary)]"
              >
                {tag}
              </span>
            ))}
          </div>
        </div>
      ) : null}

      <div className="py-3">
        <button
          type="button"
          onClick={onOpenPage}
          className="flex h-[30px] w-full items-center justify-center gap-1.5 rounded-[var(--radius-md)] bg-[var(--foreground)] text-[12px] font-medium text-[var(--text-inverse)] hover:bg-[#1a1a1a]"
        >
          <ExternalLink size={13} />
          {t("graph.inspector.openPage")}
        </button>
      </div>
    </div>
  );
}
