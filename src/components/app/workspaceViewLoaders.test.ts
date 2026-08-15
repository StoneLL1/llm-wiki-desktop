import { describe, expect, it, vi } from "vitest";

import { createWorkspaceViewLoaderRegistry } from "./workspaceViewLoaders";

describe("workspace view loader registry", () => {
  it("does not preload at startup and coalesces hover/focus requests per view", async () => {
    const loadWiki = vi.fn(async () => ({ WikiView: () => null }));
    const loadGraph = vi.fn(async () => ({ GraphView: () => null }));
    const registry = createWorkspaceViewLoaderRegistry({ wiki: loadWiki, graph: loadGraph });

    expect(loadWiki).not.toHaveBeenCalled();
    expect(loadGraph).not.toHaveBeenCalled();

    const first = registry.preload("graph");
    const second = registry.preload("graph");
    expect(first).toBe(second);
    await first;

    expect(loadGraph).toHaveBeenCalledOnce();
    expect(loadWiki).not.toHaveBeenCalled();
  });

  it("drops a rejected preload so a later attempt can recover", async () => {
    const loadGraph = vi
      .fn<() => Promise<{ GraphView: () => null }>>()
      .mockRejectedValueOnce(new Error("chunk missing"))
      .mockResolvedValueOnce({ GraphView: () => null });
    const registry = createWorkspaceViewLoaderRegistry({ graph: loadGraph });

    await expect(registry.preload("graph")).rejects.toThrow("chunk missing");
    await expect(registry.preload("graph")).resolves.toBeDefined();
    expect(loadGraph).toHaveBeenCalledTimes(2);
  });
});
