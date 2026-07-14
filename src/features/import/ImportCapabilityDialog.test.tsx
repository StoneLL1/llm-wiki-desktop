import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportCapabilityRequirement } from "../../types/importV2Presentation";
import { ImportCapabilityDialog } from "./ImportCapabilityDialog";

const requirement: ImportCapabilityRequirement = {
  requirement: {
    capabilityId: "browser-runtime",
    minimumVersion: "1.4.0",
    protocolVersion: "2",
    targetTriple: "x86_64-pc-windows-msvc",
    acceptedLicenseExpressions: ["Apache-2.0"],
  },
  route: "web.generic.browser",
  available: false,
  installable: true,
  compressedBytes: 12_000,
  installedBytes: 45_000,
  modelBytes: null,
  license: "Apache-2.0",
  fallback: "Install the signed pack from a release, then retry.",
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportCapabilityDialog", () => {
  it("shows pinned capability facts, license, target platform, and fallback", () => {
    render(<ImportCapabilityDialog open requirement={requirement} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByText(/browser-runtime/)).toBeInTheDocument();
    expect(screen.getByText(/1\.4\.0/)).toBeInTheDocument();
    expect(screen.getByText(/Apache-2\.0/)).toBeInTheDocument();
    expect(screen.getByText(/x86_64-pc-windows-msvc/)).toBeInTheDocument();
    expect(screen.getByText(/signed pack from a release/i)).toBeInTheDocument();
  });

  it("requires explicit confirmation and reports install intent without downloading", async () => {
    const onInstall = vi.fn().mockResolvedValue(undefined);
    render(<ImportCapabilityDialog open requirement={requirement} onInstall={onInstall} onCancel={vi.fn()} />);

    const install = screen.getByRole("button", { name: /install capability/i });
    expect(install).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox", { name: /understand.*signed capability/i }));
    fireEvent.click(install);
    await waitFor(() => expect(onInstall).toHaveBeenCalledWith("browser-runtime"));
  });

  it("offers fallback instead of a dead install button when the platform is unsupported", () => {
    render(<ImportCapabilityDialog open requirement={{ ...requirement, installable: false }} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.queryByRole("button", { name: /install capability/i })).not.toBeInTheDocument();
    expect(screen.getByText(/signed pack from a release/i)).toBeInTheDocument();
  });
});
