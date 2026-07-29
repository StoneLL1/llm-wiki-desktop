import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { ImportCommitBar } from "./ImportCommitBar";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportCommitBar", () => {
  it("keeps conflict resolution out of the global commit tab sequence", () => {
    const onConfirm = vi.fn();
    const { container } = render(
      <ImportCommitBar
        counts={{ newSources: 2, updates: 3, warnings: 1, pending: 4, selected: 5 }}
        isConfirming={false}
        onConfirm={onConfirm}
      />,
    );

    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(container.querySelectorAll("button, a[href], input, select, textarea, [tabindex]:not([tabindex='-1'])")).toHaveLength(1);
    expect(screen.getByText("New 2")).toBeInTheDocument();
    expect(screen.getByText("Updates 3")).toBeInTheDocument();
    expect(screen.getByText("Warnings 1")).toBeInTheDocument();
    expect(screen.getByText("Pending 4")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import to Source library (5)" })).toBeEnabled();

    fireEvent.click(screen.getByRole("button"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
