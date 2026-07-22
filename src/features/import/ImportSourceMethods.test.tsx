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

    fireEvent.drop(screen.getByRole("region", { name: /drop files or folders/i }), {
      dataTransfer: { files: [], items: [] },
    });

    expect(onAddPaths).not.toHaveBeenCalledWith([]);
  });

  it("submits a normal URL without a media choice and rejects local targets visibly", async () => {
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

  it.each([
    ["Douyin", "https://www.douyin.com/video/123", "extract_only", /save extraction only/i],
    ["Xiaohongshu", "https://www.xiaohongshu.com/explore/abc", "extract_only", /save extraction only/i],
    ["Bilibili", "https://www.bilibili.com/video/BV1xx411c7mD", "preserve_original", /save original media/i],
  ] as const)("asks for a media save mode for %s URLs", async (_platform, value, mode, choiceName) => {
    const onAddUrl = vi.fn().mockResolvedValue(undefined);
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={onAddUrl} />);
    const input = screen.getByRole("textbox", { name: "URL" });

    fireEvent.change(input, { target: { value } });
    fireEvent.click(screen.getByRole("button", { name: "Add URL" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(onAddUrl).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: choiceName }));
    await waitFor(() => expect(onAddUrl).toHaveBeenCalledWith(value, mode));
  });

  it("recognizes XHS short links as media platform URLs", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} />);
    const input = screen.getByRole("textbox", { name: "URL" });

    fireEvent.change(input, { target: { value: "https://xhslink.com/a/abc" } });
    fireEvent.click(screen.getByRole("button", { name: "Add URL" }));

    expect(screen.getByRole("dialog")).toBeInTheDocument();
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

  it("shows localized validation for malformed public URLs", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} />);
    const input = screen.getByRole("textbox", { name: "URL" });
    fireEvent.change(input, { target: { value: "not-a-url" } });

    expect(screen.getByText(/enter a public http or https url/i)).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("button", { name: "Add URL" })).toBeDisabled();
  });

  it("locks file entry points while a discovery task is running", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} addingPaths />);

    expect(screen.getByRole("button", { name: /choose files/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /choose folder/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /choose files/i })).toBeDisabled();
    expect(screen.getAllByText(/adding/i).length).toBeGreaterThan(0);
  });

  it("locks URL entry while another source addition is in flight", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} addingUrl />);

    expect(screen.getByRole("textbox", { name: "URL" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add URL" })).toBeDisabled();
  });

  it("marks phase-two connectors as unavailable", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} />);

    expect(screen.getByLabelText("HTTP: available")).toBeInTheDocument();
    expect(screen.getByLabelText("WeChat: available")).toBeInTheDocument();
    expect(screen.getByLabelText("Xiaohongshu: unavailable")).toBeInTheDocument();
  });

  it("explains a capability-gated platform without blocking other sources", () => {
    render(
      <ImportSourceMethods
        onAddPaths={vi.fn()}
        onAddUrl={vi.fn()}
        platforms={[{ label: "Zhihu", available: false, reasonCode: "capability_missing" }]}
      />,
    );

    expect(screen.getByTitle(/install the required capability pack/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose files" })).toBeEnabled();
  });
});
