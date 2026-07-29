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

function expandMatrix() {
  fireEvent.click(screen.getByRole("button", { name: /^Expand supported sources\./ }));
}

describe("ImportSourceMethods", () => {
  it("uses backend file readiness instead of hardcoded availability", () => {
    render(<ImportSourceMethods
      onAddPaths={vi.fn()}
      onAddUrl={vi.fn()}
      files={[{ id: "pdf", label: "PDF", available: false, reasonCode: "capability_missing" }]}
    />);

    expandMatrix();
    expect(screen.getByLabelText("PDF: Install the required capability pack first"))
      .toHaveClass("is-off");
  });

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
    ["Douyin", "https://www.douyin.com/video/123"],
    ["Xiaohongshu", "https://www.xiaohongshu.com/explore/abc"],
    ["Bilibili", "https://www.bilibili.com/video/BV1xx411c7mD"],
    ["XHS short link", "https://xhslink.com/a/abc"],
  ] as const)("queues %s URLs directly with the extraction-only backend default", async (_platform, value) => {
    const onAddUrl = vi.fn().mockResolvedValue(undefined);
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={onAddUrl} />);
    const input = screen.getByRole("textbox", { name: "URL" });

    fireEvent.change(input, { target: { value } });
    fireEvent.click(screen.getByRole("button", { name: "Add URL" }));
    await waitFor(() => expect(onAddUrl).toHaveBeenCalledWith(value));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("does not leave media-choice controls in the keyboard sequence", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} />);

    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "URL" })).toHaveProperty("tabIndex", 0);
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

  it("previews pasted Markdown before explicitly adding it", async () => {
    const onAddText = vi.fn().mockResolvedValue(undefined);
    render(
      <ImportSourceMethods
        onAddPaths={vi.fn()}
        onAddText={onAddText}
        onAddUrl={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Paste text or Markdown" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Text or Markdown" }), {
      target: { value: "# Local notes\n\nBody" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "Source name" }), {
      target: { value: "notes.md" },
    });

    expect(screen.getByRole("region", { name: "Clipboard preview" })).toHaveTextContent("Local notes");
    expect(screen.getByText("Detected as Markdown")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Add pasted text" }));

    await waitFor(() => {
      expect(onAddText).toHaveBeenCalledWith("# Local notes\n\nBody", "notes.md");
    });
  });

  it("rejects pasted images without losing existing text", () => {
    render(
      <ImportSourceMethods
        onAddPaths={vi.fn()}
        onAddText={vi.fn()}
        onAddUrl={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Paste text or Markdown" }));
    const textarea = screen.getByRole("textbox", { name: "Text or Markdown" });
    fireEvent.change(textarea, { target: { value: "keep me" } });
    fireEvent.paste(textarea, {
      clipboardData: {
        items: [{ kind: "file", type: "image/png" }],
      },
    });

    expect(screen.getByText(/image paste is not supported/i)).toBeInTheDocument();
    expect(textarea).toHaveValue("keep me");
  });

  it("keeps the four primary source entries compact and the capability matrix separate", () => {
    const view = render(<ImportSourceMethods onAddPaths={vi.fn()} onAddText={vi.fn()} onAddUrl={vi.fn()} />);

    expect(screen.getByRole("button", { name: "Choose files" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose folder" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "URL" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Paste text or Markdown" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("textbox", { name: "Text or Markdown" })).not.toBeInTheDocument();
    expect(view.container.querySelector(".import-v2-method-pane")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Supported files, platforms, and extraction capabilities")).toBeInTheDocument();
    const matrixSummary = screen.getByRole("button", { name: /^Expand supported sources\./ });
    expect(matrixSummary).toHaveAttribute("aria-expanded", "false");
    expect(matrixSummary).toHaveAccessibleName(/Files: 7\/7 available/);
    expect(matrixSummary).toHaveAccessibleName(/Platforms: 2\/7 available/);
    expect(matrixSummary).toHaveAccessibleName(/Abilities: 0\/0 available/);
    expect(screen.queryByLabelText("HTTP: available")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "URL" }).closest("form")).toHaveClass("import-v2-compact-url");

    fireEvent.click(screen.getByRole("button", { name: "Collapse add methods" }));
    expect(screen.queryByRole("textbox", { name: "URL" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Expand add methods" }));
    expect(screen.getByRole("textbox", { name: "URL" })).toBeInTheDocument();

    expandMatrix();
    expect(screen.getByRole("button", { name: /^Collapse supported sources\./ })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByLabelText("HTTP: available")).toBeInTheDocument();
  });

  it("shows localized validation for malformed public URLs", () => {
    render(<ImportSourceMethods onAddPaths={vi.fn()} onAddUrl={vi.fn()} />);
    const input = screen.getByRole("textbox", { name: "URL" });
    fireEvent.change(input, { target: { value: "not-a-url" } });

    expect(screen.getByText(/enter a public http or https url/i)).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(input).toHaveAttribute("aria-describedby", "import-v2-url-feedback");
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

    expandMatrix();
    expect(screen.getByLabelText("HTTP: available")).toBeInTheDocument();
    expect(screen.getByLabelText("WeChat: available")).toBeInTheDocument();
    expect(screen.getByLabelText("Xiaohongshu: unavailable")).toBeInTheDocument();
  });

  it("supports focus, Enter, Escape, and outside-click semantics for capability tiles", async () => {
    const onManageCapabilities = vi.fn();
    render(
      <ImportSourceMethods
        onAddPaths={vi.fn()}
        onAddUrl={vi.fn()}
        platforms={[{ id: "zhihu", label: "Zhihu", available: false, reasonCode: "capability_missing" }]}
        onManageCapabilities={onManageCapabilities}
      />,
    );

    expandMatrix();
    const tile = screen.getByRole("button", { name: /Zhihu: Install the required capability pack first/i });
    tile.focus();
    expect(await screen.findByRole("tooltip")).toHaveTextContent(/install the required capability pack/i);
    fireEvent.click(tile);
    expect(screen.getByRole("dialog")).toHaveTextContent(/install the required capability pack/i);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(tile).toHaveFocus();

    fireEvent.click(tile);
    fireEvent.pointerDown(document.body);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose files" })).toBeEnabled();
  });

  it("renders backend readiness for platforms and media abilities", () => {
    render(
      <ImportSourceMethods
        onAddPaths={vi.fn()}
        onAddUrl={vi.fn()}
        platforms={[{ id: "bilibili", label: "Bilibili", available: true }]}
        abilities={[
          { id: "subtitle", label: "Subtitles", available: true },
          { id: "local_asr", label: "Local ASR", available: false, reasonCode: "capability_missing" },
        ]}
      />,
    );

    expandMatrix();
    expect(screen.getByLabelText("Bilibili: available")).toBeInTheDocument();
    expect(screen.getByLabelText("Subtitles: available")).toBeInTheDocument();
    expect(screen.getByLabelText(/Local ASR: Install the required capability pack first/i)).toBeInTheDocument();
    expect(screen.getByLabelText("Supported files, platforms, and extraction capabilities")).toBeInTheDocument();
  });
});
