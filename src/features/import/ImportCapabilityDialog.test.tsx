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
  unavailableReasonCode: null,
  requirementRevision: "ab".repeat(32),
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportCapabilityDialog", () => {
  it("shows pinned capability facts, license, target platform, and fallback", () => {
    render(<ImportCapabilityDialog open requirement={requirement} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByText("Interactive web reader")).toBeVisible();
    expect(screen.getByText("Read dynamic web sources")).toBeVisible();
    expect(screen.getByText(/1\.4\.0/)).toBeInTheDocument();
    expect(screen.getByText(/Apache-2\.0/)).toBeInTheDocument();
    expect(screen.getByText("browser-runtime")).not.toBeVisible();
    expect(screen.getByText("web.generic.browser")).not.toBeVisible();
    expect(screen.getByText("x86_64-pc-windows-msvc")).not.toBeVisible();
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

  it("keeps the install entry visible but disabled when no trusted installer is available", () => {
    render(<ImportCapabilityDialog open requirement={{ ...requirement, installable: false }} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByRole("button", { name: /install capability/i })).toBeDisabled();
    expect(screen.getByText(/signed pack from a release/i)).toBeInTheDocument();
  });

  it("labels a development build with an empty catalog explicitly", () => {
    render(<ImportCapabilityDialog open requirement={{ ...requirement, installable: false, unavailableReasonCode: "catalog_unavailable" }} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByRole("alert")).toHaveTextContent(/development build.*catalog/i);
  });

  it("explains that installation is read-only while runner confinement is unverified", () => {
    render(<ImportCapabilityDialog open requirement={{ ...requirement, installable: false, unavailableReasonCode: "runtime_confinement_unavailable" }} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByRole("alert")).toHaveTextContent(/installation remains read-only.*confinement/i);
    expect(screen.getByRole("button", { name: /install capability/i })).toBeDisabled();
  });
});
