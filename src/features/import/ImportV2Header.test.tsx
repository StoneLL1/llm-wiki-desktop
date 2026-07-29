import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { i18next } from "../../i18n";
import { ImportV2Header } from "./ImportV2Header";

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportV2Header", () => {
  it("keeps the page title semantic-only and places the section navigation first", () => {
    const view = render(
      <ImportV2Header
        session={null}
        activeSection="workbench"
      />,
    );

    expect(screen.getByRole("heading", { level: 1, name: "Import" })).toHaveClass("sr-only");
    expect(screen.queryByText("Turn sources into readable Markdown while keeping originals immutable."))
      .not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Workbench" })).toHaveAttribute("aria-current", "page");
    expect(view.container.querySelector(".import-v2-header__stat")).toHaveAttribute("aria-live", "polite");
    expect(view.container.querySelector(".import-v2-header__tools")?.firstElementChild)
      .toHaveClass("import-v2-header__nav");
  });
});
