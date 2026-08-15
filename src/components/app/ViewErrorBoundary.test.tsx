import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import "../../i18n";
import { ViewErrorBoundary } from "./ViewErrorBoundary";

describe("ViewErrorBoundary", () => {
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});

  beforeEach(() => {
    consoleError.mockClear();
  });

  afterEach(() => {
    consoleError.mockClear();
  });

  it("offers a real application reload for a rejected lazy identity", () => {
    const reload = vi.fn();

    function ThrowingView(): null {
      throw new Error("view chunk failed");
    }

    render(
      <ViewErrorBoundary onReload={reload}>
        <ThrowingView />
      </ViewErrorBoundary>,
    );

    expect(screen.getByRole("alert")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /reload|retry/i }));

    expect(reload).toHaveBeenCalledOnce();
  });
});
