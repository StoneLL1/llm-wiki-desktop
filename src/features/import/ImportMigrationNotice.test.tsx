import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportFrontendReadiness } from "../../types/importV2Presentation";
import { ImportMigrationNotice } from "./ImportMigrationNotice";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportMigrationNotice", () => {
  it("blocks the import entry until backend readiness is active and offers migration review", () => {
    const readiness: ImportFrontendReadiness = {
      backendVersion: "2.0.0",
      active: false,
      migrationStatus: "awaiting_confirmation",
      unfinishedSessionId: null,
      legacyHistoryAvailable: true,
    };
    const onOpenMigration = vi.fn();
    render(<ImportMigrationNotice readiness={readiness} onOpenMigration={onOpenMigration} />);

    expect(screen.getByRole("alert")).toHaveTextContent(/migration.*confirmation/i);
    fireEvent.click(screen.getByRole("button", { name: /review migration/i }));
    expect(onOpenMigration).toHaveBeenCalledTimes(1);
  });

  it("does not show an emergency V1 switch after activation", () => {
    const readiness: ImportFrontendReadiness = {
      backendVersion: "2.0.0",
      active: true,
      migrationStatus: "applied",
      unfinishedSessionId: null,
      legacyHistoryAvailable: false,
    };
    render(<ImportMigrationNotice readiness={readiness} onOpenMigration={vi.fn()} />);
    expect(screen.queryByRole("button", { name: /switch to V1|legacy writes/i })).not.toBeInTheDocument();
  });
});
