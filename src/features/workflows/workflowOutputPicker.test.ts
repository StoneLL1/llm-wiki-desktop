import { beforeEach, describe, expect, it, vi } from "vitest";
import { pickWorkflowOutputPath, workflowOutputRelativePath } from "./workflowOutputPicker";

const save = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ save }));
beforeEach(() => save.mockReset());

describe("workflow output file selection", () => {
  it.each([
    ["/知识库/Café", "exports/html", "/知识库/Café/exports/html/报告.html", "exports/html/报告.html"],
    ["C:\\知识库", "exports/html", "c:\\知识库\\exports\\html\\报告.html", "exports/html/报告.html"],
    ["\\\\server\\share\\Wiki", "Output/HTML", "\\\\SERVER\\share\\Wiki\\Output\\HTML\\Café.html", "Output/HTML/Café.html"],
    ["/vault/", "分享/网页", "/vault/分享/网页/报告.HTML", "分享/网页/报告.HTML"],
    ["//?/C:/知识库", "exports/html", "C:\\知识库\\exports\\html\\报告.html", "exports/html/报告.html"],
    ["//?/UNC/server/share/Wiki", "exports/html", "\\\\server\\share\\Wiki\\exports\\html\\报告.html", "exports/html/报告.html"],
  ])("converts %s without changing the chosen filename", (root, exportRoot, selected, expected) => {
    expect(workflowOutputRelativePath(selected, root, exportRoot)).toBe(expected);
  });

  it.each([
    "/vault-other/exports/html/report.html",
    "/Vault/exports/html/report.html",
    "/vault/raw/report.html",
    "/vault/exports/html-other/report.html",
    "/vault/exports/html/../report.html",
    "/vault/exports/html//report.html",
    "/vault/exports/html/report.txt",
  ])("rejects an invalid destination %s", (path) => {
    expect(() => workflowOutputRelativePath(path, "/vault", "exports/html")).toThrow();
  });

  it("opens the system save dialog in the layout-defined export directory", async () => {
    save.mockResolvedValue("/知识库/分享/网页/报告.html");
    await expect(pickWorkflowOutputPath({ projectRoot: "/知识库", exportRoot: "分享/网页", currentPath: "", title: "保存 HTML" })).resolves.toBe("分享/网页/报告.html");
    expect(save).toHaveBeenCalledWith({ title: "保存 HTML", defaultPath: "/知识库/分享/网页/report.html", filters: [{ name: "HTML", extensions: ["html"] }] });
  });

  it("reopens the chosen file and returns null on cancellation", async () => {
    save.mockResolvedValue(null);
    await expect(pickWorkflowOutputPath({ projectRoot: "/vault", exportRoot: "exports/html", currentPath: "exports/html/Café.html", title: "Save" })).resolves.toBeNull();
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: "/vault/exports/html/Café.html" }));
  });

  it("does not invent an export directory for a compatible vault", async () => {
    await expect(pickWorkflowOutputPath({ projectRoot: "/vault", currentPath: "", title: "Save" })).rejects.toThrow("exportRootUnavailable");
    expect(save).not.toHaveBeenCalled();
  });
});
