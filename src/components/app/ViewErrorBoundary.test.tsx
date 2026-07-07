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

  it("retries the failed view locally instead of reloading the whole app", () => {
    let shouldThrow = true;

    function MaybeThrowingView() {
      if (shouldThrow) throw new Error("view chunk failed");
      return <div>Recovered view</div>;
    }

    render(
      <ViewErrorBoundary>
        <MaybeThrowingView />
      </ViewErrorBoundary>,
    );

    expect(screen.getByRole("alert")).toBeInTheDocument();

    shouldThrow = false;
    fireEvent.click(screen.getByRole("button", { name: /reload|retry/i }));

    expect(screen.getByText("Recovered view")).toBeInTheDocument();
  });
});
