import type { TFunction } from "i18next";

export function capabilityDisplayName(capabilityId: string, t: TFunction): string {
  const fallback = capabilityId
    .split(/[-_.]+/)
    .filter(Boolean)
    .map((part) => part[0]?.toLocaleUpperCase() + part.slice(1))
    .join(" ");
  return t(`importV2.capabilityName.${capabilityId}`, {
    defaultValue: fallback || t("importV2.capabilityName.unknown"),
  });
}

export function capabilityPurpose(route: string, t: TFunction): string {
  if (route.startsWith("web.")) return t("importV2.capabilityPurpose.web");
  if (route === "media.asr") return t("importV2.capabilityPurpose.asr");
  if (route.startsWith("media.")) return t("importV2.capabilityPurpose.media");
  if (route.startsWith("ocr.")) return t("importV2.capabilityPurpose.ocr");
  if (route === "pdf.layout") return t("importV2.capabilityPurpose.pdfLayout");
  if (route.startsWith("pack.")) return t("importV2.capabilityPurpose.documents");
  return t("importV2.capabilityPurpose.local");
}
