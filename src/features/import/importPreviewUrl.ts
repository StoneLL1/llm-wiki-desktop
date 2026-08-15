import type { ImportPreviewResource } from "../../types/importV2Presentation";

export function safeExternalUrl(value: string | undefined): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:" || url.protocol === "mailto:"
      ? url.toString()
      : null;
  } catch {
    return null;
  }
}

export function previewImageUrl(
  source: string | undefined,
  resources: readonly ImportPreviewResource[],
): string | null {
  if (!source) return null;
  const normalized = source.replace(/\\/g, "/").replace(/^\.\//, "");
  const name = normalized.split("/").at(-1);
  const resource = resources.find((candidate) =>
    candidate.kind === "image"
    && (
      candidate.source.replace(/\\/g, "/") === normalized
      || candidate.source.replace(/\\/g, "/").endsWith(`/${normalized}`)
      || candidate.name === name
    ));
  return resource?.dataUrl?.startsWith("data:image/") ? resource.dataUrl : null;
}
