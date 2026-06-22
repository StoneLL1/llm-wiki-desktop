import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { useImportStore } from "../../stores/importStore";
import { ImportView } from "./ImportView";

const openDialog = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: openDialog }));

beforeEach(async () => {
  openDialog.mockReset();
  useImportStore.getState().reset();
  await i18next.changeLanguage("en");
});

describe("ImportView native file selection", () => {
  it("previews the files selected from the Local files card", async () => {
    const onRequestPreview = vi.fn();
    openDialog.mockResolvedValue(["D:\\资料\\论文.pdf", "C:\\Notes\\研究.docx"]);

    render(
      <ImportView
        isConfirming={false}
        onRequestPreview={onRequestPreview}
        onRequestClipboard={vi.fn()}
        onRequestUrl={vi.fn()}
        importedSources={[]}
        onDeleteSource={vi.fn()}
        onReplaceSource={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Local files" }));

    await waitFor(() => {
      expect(onRequestPreview).toHaveBeenCalledWith([
        "D:\\资料\\论文.pdf",
        "C:\\Notes\\研究.docx",
      ]);
    });
  });
});
