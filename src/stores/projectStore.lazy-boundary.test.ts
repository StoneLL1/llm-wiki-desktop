import { describe, expect, it, vi } from "vitest";

vi.mock("../features/wiki/wikiStore", () => { throw new Error("wiki store evaluated"); });
vi.mock("../features/wiki/sourceStore", () => { throw new Error("source store evaluated"); });
vi.mock("./chatStore", () => { throw new Error("chat store evaluated"); });
vi.mock("./exportStore", () => { throw new Error("export store evaluated"); });
vi.mock("./graphStore", () => { throw new Error("graph store evaluated"); });
vi.mock("./importStore", () => { throw new Error("import store evaluated"); });
vi.mock("./lintStore", () => { throw new Error("lint store evaluated"); });
vi.mock("./settingsStore", () => { throw new Error("settings store evaluated"); });
vi.mock("./workflowStore", () => { throw new Error("workflow store evaluated"); });

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("projectStore lazy boundary", () => {
  it("does not evaluate feature stores through the project reset path", async () => {
    await expect(import("./projectStore")).resolves.toHaveProperty("useProjectStore");
  });
});
