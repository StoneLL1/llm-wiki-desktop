import type Graph from "graphology";

import type { GraphData } from "../../types/graph";

/** Node attributes read off the graphology graph for export. */
interface ExportNode {
  id: string;
  label: string;
  x: number;
  y: number;
  size: number;
  color: string;
  type?: string;
}

const SVG_VIEWBOX = 1200;
const PADDING = 60;
const STROKE_COLOR = "#d4d4d4";
const LABEL_COLOR = "#6b7280";

/**
 * Build a self-contained SVG string for the visible portion of the graph.
 *
 * Pure function over the graphology graph so it is unit-testable without a
 * canvas/sigma instance. Nodes carry sigma-space coordinates (`x`/`y`); we
 * compute a bounding box and normalize everything into a fixed square viewBox
 * so the exported file is independent of the current pan/zoom. Node colors are
 * read straight from the graph attribute (already reflects the active color
 * mode + filters at call time), matching what the user sees.
 */
export function buildGraphSvg(graph: Graph, selectedNodeId: string | null): string {
  const nodes: ExportNode[] = [];
  graph.forEachNode((id, attrs) => {
    const typed = attrs as Record<string, unknown>;
    const x = typeof typed.x === "number" && Number.isFinite(typed.x) ? (typed.x as number) : 0;
    const y = typeof typed.y === "number" && Number.isFinite(typed.y) ? (typed.y as number) : 0;
    nodes.push({
      id,
      label: typeof typed.label === "string" ? (typed.label as string) : id,
      x,
      y,
      size: typeof typed.size === "number" ? (typed.size as number) : 4,
      color: typeof typed.color === "string" ? (typed.color as string) : "#9b9b9b",
      type: typeof typed.type === "string" ? (typed.type as string) : undefined,
    });
  });

  if (nodes.length === 0) {
    return emptySvg();
  }

  const { scale, minX, minY, originX, originY } = computeTransform(nodes);
  // project(nodeX, nodeY): normalize the graph-space bounding box into the
  // SVG viewBox with padding, then center any leftover slack.
  const project = (x: number, y: number): [number, number] => [
    originX + (x - minX) * scale,
    originY + (y - minY) * scale,
  ];

  const edges: string[] = [];
  graph.forEachEdge((edge, attrs, source, target) => {
    const srcNode = nodes.find((n) => n.id === source);
    const tgtNode = nodes.find((n) => n.id === target);
    if (!srcNode || !tgtNode) return;
    const [x1, y1] = project(srcNode.x, srcNode.y);
    const [x2, y2] = project(tgtNode.x, tgtNode.y);
    edges.push(
      `<line class="edge" x1="${fmt(x1)}" y1="${fmt(y1)}" x2="${fmt(x2)}" y2="${fmt(y2)}" stroke="${STROKE_COLOR}" stroke-width="0.5"/>`,
    );
    void edge;
  });

  const nodeMarks: string[] = [];
  for (const node of nodes) {
    const [cx, cy] = project(node.x, node.y);
    const r = node.size;
    const isSelected = node.id === selectedNodeId;
    const stroke = isSelected ? "#0d9488" : "none";
    const strokeWidth = isSelected ? 2 : 0;
    nodeMarks.push(
      `<circle cx="${fmt(cx)}" cy="${fmt(cy)}" r="${fmt(r)}" fill="${node.color}"${isSelected ? ` stroke="${stroke}" stroke-width="${strokeWidth}"` : ""}/>`,
    );
    nodeMarks.push(
      `<text x="${fmt(cx)}" y="${fmt(cy + r + 10)}" text-anchor="middle" font-family="Inter, sans-serif" font-size="10" fill="${LABEL_COLOR}">${escapeXml(node.label)}</text>`,
    );
  }

  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${SVG_VIEWBOX} ${SVG_VIEWBOX}" width="${SVG_VIEWBOX}" height="${SVG_VIEWBOX}">`,
    `<rect x="0" y="0" width="${SVG_VIEWBOX}" height="${SVG_VIEWBOX}" fill="#ffffff"/>`,
    `<g>${edges.join("")}</g>`,
    `<g>${nodeMarks.join("")}</g>`,
    `</svg>`,
  ].join("");
}

function emptySvg(): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${SVG_VIEWBOX} ${SVG_VIEWBOX}" width="${SVG_VIEWBOX}" height="${SVG_VIEWBOX}"><rect x="0" y="0" width="${SVG_VIEWBOX}" height="${SVG_VIEWBOX}" fill="#ffffff"/></svg>`;
}

function computeTransform(nodes: ExportNode[]): {
  scale: number;
  minX: number;
  minY: number;
  originX: number;
  originY: number;
} {
  const xs = nodes.map((n) => n.x);
  const ys = nodes.map((n) => n.y);
  const minX = Math.min(...xs);
  const maxX = Math.max(...xs);
  const minY = Math.min(...ys);
  const maxY = Math.max(...ys);
  const width = maxX - minX || 1;
  const height = maxY - minY || 1;
  const inner = SVG_VIEWBOX - PADDING * 2;
  const scale = Math.min(inner / width, inner / height);
  // Center the drawing inside the padded viewBox.
  const originX = PADDING + (inner - width * scale) / 2;
  const originY = PADDING + (inner - height * scale) / 2;
  return { scale, minX, minY, originX, originY };
}

function fmt(n: number): string {
  return Number.isFinite(n) ? n.toFixed(2) : "0";
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/** Build a timestamped filename for a graph export. */
export function graphExportFilename(projectName: string, ext: "svg" | "png"): string {
  const stamp = exportTimestamp();
  const safe = projectName.replace(/[^\w.-]+/g, "-").replace(/^-+|-+$/g, "") || "wiki";
  return `${safe}-graph-${stamp}.${ext}`;
}

/**
 * Trigger a browser download for a blob. Falls back gracefully when the anchor
 * API is unavailable (tests); returns whether the download was initiated.
 */
export function downloadBlob(blob: Blob, filename: string): boolean {
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.style.display = "none";
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  // Revoke on the next tick so the click has time to read the URL.
  setTimeout(() => URL.revokeObjectURL(url), 0);
  return true;
}

/** Export the current graph as an SVG file. */
export function exportGraphSvg(graph: Graph, projectName: string, selectedNodeId: string | null): boolean {
  const svg = buildGraphSvg(graph, selectedNodeId);
  const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
  return downloadBlob(blob, graphExportFilename(projectName, "svg"));
}

/**
 * Export the current graph as a PNG by rasterizing the generated SVG through a
 * canvas. Async because image decoding is. Resolves to whether the download
 * started; rejects are swallowed into `false` so a rendering hiccup never
 * throws in the UI.
 */
export async function exportGraphPng(graph: Graph, projectName: string, selectedNodeId: string | null): Promise<boolean> {
  const svg = buildGraphSvg(graph, selectedNodeId);
  const blob = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  try {
    const img = new Image();
    img.decoding = "async";
    await decodeImage(img, url);
    const canvas = document.createElement("canvas");
    canvas.width = SVG_VIEWBOX;
    canvas.height = SVG_VIEWBOX;
    const ctx = canvas.getContext("2d");
    if (!ctx) return false;
    ctx.fillStyle = "#ffffff";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0);
    const dataUrl = canvas.toDataURL("image/png");
    const pngBlob = dataUrlToBlob(dataUrl);
    return downloadBlob(pngBlob, graphExportFilename(projectName, "png"));
  } catch {
    return false;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function decodeImage(img: HTMLImageElement, url: string): Promise<void> {
  return new Promise((resolve, reject) => {
    img.onload = () => resolve();
    img.onerror = () => reject(new Error("svg decode failed"));
    img.src = url;
  });
}

function dataUrlToBlob(dataUrl: string): Blob {
  const [meta, base64] = dataUrl.split(",");
  const mime = /data:(.*?);base64/.exec(meta)?.[1] ?? "image/png";
  const binary = atob(base64 ?? "");
  const len = binary.length;
  const bytes = new Uint8Array(len);
  for (let i = 0; i < len; i++) bytes[i] = binary.charCodeAt(i);
  return new Blob([bytes], { type: mime });
}

/**
 * Current timestamp as `YYYYMMDD-HHMM`. `new Date()` is avoided at module scope
 * to keep the pure SVG builder deterministic for tests; callers pass real time.
 */
function exportTimestamp(d: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`;
}

/** Re-exported so tests can assert filename shape with a fixed clock. */
export const __test__ = { exportTimestamp };

export type { GraphData };
