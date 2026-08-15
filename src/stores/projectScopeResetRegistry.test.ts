import { describe, expect, it, vi } from "vitest";

import { createProjectScopeResetRegistry } from "./projectScopeResetRegistry";

describe("projectScopeResetRegistry", () => {
  it("runs a stable snapshot and keeps later handlers alive when one fails", () => {
    const registry = createProjectScopeResetRegistry();
    const calls: string[] = [];

    registry.register("first", () => {
      calls.push("first");
      registry.register("late", () => {
        calls.push("late");
      });
      throw new Error("best-effort cancellation failed");
    });
    registry.register("second", () => {
      calls.push("second");
    });

    expect(() => registry.reset()).not.toThrow();
    expect(calls).toEqual(["first", "second"]);

    registry.reset();
    expect(calls).toEqual(["first", "second", "first", "second", "late"]);
  });

  it("supports replacement and identity-safe unsubscribe for tests and HMR", () => {
    const registry = createProjectScopeResetRegistry();
    const first = vi.fn();
    const replacement = vi.fn();
    const unsubscribeFirst = registry.register("wiki", first);
    registry.register("wiki", replacement);

    unsubscribeFirst();
    registry.reset();

    expect(first).not.toHaveBeenCalled();
    expect(replacement).toHaveBeenCalledOnce();
  });
});
