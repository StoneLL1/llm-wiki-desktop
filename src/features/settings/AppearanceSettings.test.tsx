import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import "../../i18n";
import { AppearanceSettings } from "./AppearanceSettings";

describe("AppearanceSettings", () => {
  it("renders built-in color presets without arbitrary color inputs", () => {
    render(
      <AppearanceSettings
        theme="auto"
        colorThemePreset="codex"
        onChange={vi.fn()}
        onChangeColorThemePreset={vi.fn()}
      />,
    );

    expect(screen.getByRole("radio", { name: /Codex/i })).toHaveAttribute("aria-checked", "true");
    expect(screen.getAllByRole("radio").length).toBeGreaterThanOrEqual(4);
    expect(screen.queryByLabelText(/hex|rgb|hsl|css variable/i)).not.toBeInTheDocument();
  });

  it("notifies when a preset is selected", () => {
    const onChangeColorThemePreset = vi.fn();
    render(
      <AppearanceSettings
        theme="auto"
        colorThemePreset="codex"
        onChange={vi.fn()}
        onChangeColorThemePreset={onChangeColorThemePreset}
      />,
    );

    fireEvent.click(screen.getByRole("radio", { name: /Paper/i }));
    expect(onChangeColorThemePreset).toHaveBeenCalledWith("paper");
  });
});
