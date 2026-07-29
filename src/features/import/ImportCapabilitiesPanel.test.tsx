import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { i18next } from "../../i18n";
import { ImportCapabilitiesPanel } from "./ImportCapabilitiesPanel";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportCapabilitiesPanel", () => {
  it("summarizes typed capability state without leaking routes or reason codes", () => {
    render(<ImportCapabilitiesPanel capabilities={[
      { capabilityId: "media-pack", route: "media.asr", available: true },
      { capabilityId: "media-pack", route: "media.capture", available: false, reasonCode: "IMPORT_V2_CAPABILITY_MISSING" },
    ]} />);

    expect(screen.getByText("Partial")).toBeInTheDocument();
    expect(screen.getByText("Supported source types: 2")).toBeInTheDocument();
    expect(screen.queryByText(/media\.asr/)).not.toBeInTheDocument();
    expect(screen.queryByText(/IMPORT_V2_CAPABILITY_MISSING/)).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Media Pack" })).toBeVisible();
    expect(screen.queryByText("media-pack")).not.toBeInTheDocument();
    expect(screen.getByText("0/1 available")).toBeInTheDocument();
  });
});
