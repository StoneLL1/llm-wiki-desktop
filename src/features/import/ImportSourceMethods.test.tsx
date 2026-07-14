import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";

const openDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog }));

import { ImportSourceMethods } from "./ImportSourceMethods";

beforeEach(async () => {
  openDialog.mockReset();
  await i18next.changeLanguage("en");
});

describe("ImportSourceMethods", () => {
  it("adds multiple selected files and folders as path strings", async () => {
    openDialog
      .mockResolvedValueOnce(["D:\\资料\\研究.pdf", "C:\\Notes\\研究.docx"])
      .mockResolvedValueOnce("D:\\资料\\资料集");
    const onAddPaths = vi.fn();

    render(<ImportSourceMethods onAddPaths={onAddPaths} onAddUrl={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "Choose files" }));
    await waitFor(() => expect(onAddPaths).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: "Choose folder" }));

    await waitFor(() => {
      expect(onAddPaths).toHaveBeenNthCalledWith(2, ["D:\\资料\\资料集"]);
    });
  });

  it("ignores browser drop payloads without Tauri-authorized paths", () => {
    const onAddPaths = vi.fn();
    render(<ImportSourceMethods onAddPaths={onAddPaths} onAddUrl={vi.fn()} />);

    fireEvent.drop(screen.getByRole("button", { name: /drop files or folders/i }), {
      dataTransfer: { files: [], items: [] },
    });

    expect(onAddPaths).not.toHaveBeenCalledWith([]);
  });

  it("submits a URL on Enter, clears only after acceptance, and rejects local targets visibly", async () => {
    const onAddUrl = vi.fn().mockResolvedValue(undefined);
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={onAddUrl} />);
    const input = screen.getByRole("textbox", { name: "URL" });

    fireEvent.change(input, { target: { value: "https://example.com/article" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(onAddUrl).toHaveBeenCalledWith("https://example.com/article"));
    expect(input).toHaveValue("");

    fireEvent.change(input, { target: { value: "file:///private/note.md" } });
    expect(screen.getByText(/local URLs are not supported/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add URL" })).toBeDisabled();
  });

  it("keeps a rejected URL for retry", async () => {
    const onAddUrl = vi.fn().mockRejectedValue(new Error("rejected"));
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={onAddUrl} />);
    const input = screen.getByRole("textbox", { name: "URL" });

    fireEvent.change(input, { target: { value: "https://example.com/retry" } });
    fireEvent.click(screen.getByRole("button", { name: "Add URL" }));
    await waitFor(() => expect(onAddUrl).toHaveBeenCalled());
    expect(input).toHaveValue("https://example.com/retry");
  });

  it("marks phase-two connectors as unavailable", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} />);

    expect(screen.getByLabelText("HTTP: available")).toBeInTheDocument();
    expect(screen.getByLabelText("Xiaohongshu: unavailable")).toBeInTheDocument();
  });
});
