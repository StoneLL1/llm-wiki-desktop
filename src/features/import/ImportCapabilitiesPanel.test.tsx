import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { i18next } from "../../i18n";
import { ImportCapabilitiesPanel } from "./ImportCapabilitiesPanel";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportCapabilitiesPanel", () => {
  it("shows each route and marks a mixed capability as partial", () => {
    render(<ImportCapabilitiesPanel capabilities={[
      { capabilityId: "media-pack", route: "media.asr", available: true },
      { capabilityId: "media-pack", route: "media.capture", available: false, reasonCode: "IMPORT_V2_CAPABILITY_MISSING" },
    ]} />);

    expect(screen.getByText("Partial")).toBeInTheDocument();
    expect(screen.getByText(/media\.asr/)).toBeInTheDocument();
    expect(screen.getByText(/IMPORT_V2_CAPABILITY_MISSING/)).toBeInTheDocument();
    expect(screen.getByText("0/1 available")).toBeInTheDocument();
  });
});
