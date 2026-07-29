import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportThreeWayMergeContext } from "../../types/importV2";
import { ImportMergeResolutionDialog } from "./ImportMergeResolutionDialog";

const mergeContext: ImportThreeWayMergeContext = {
  resolution: {
    kind: "needs_three_way_merge",
    binding: {
      sourceId: "source-internal",
      candidateHash: "candidate-hash",
      currentHash: "current-hash",
      targetVersionId: "version-internal",
    },
  },
  baselineMarkdown: "# Previously imported",
  currentMarkdown: "# 当前 Source\n\n人工编辑",
  candidateMarkdown: "# 导入更新\n\n新内容",
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportMergeResolutionDialog", () => {
  it("shows the current/imported/merged relationship without exposing internal bindings", async () => {
    render(
      <ImportMergeResolutionDialog
        open
        itemId="item-a"
        title="研究笔记.md"
        loadContext={vi.fn().mockResolvedValue(mergeContext)}
        onChoose={vi.fn()}
        onSaveMerged={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(await screen.findByText("Existing Source")).toBeInTheDocument();
    expect(screen.getByText("Imported update")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Merged result" })).toHaveValue(
      mergeContext.candidateMarkdown,
    );
    expect(screen.queryByText(/source-internal|version-internal|candidate-hash|baseline/i)).not.toBeInTheDocument();
  });

  it("persists one typed decision for the current item", async () => {
    const onChoose = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    render(
      <ImportMergeResolutionDialog
        open
        itemId="item-a"
        title="notes.md"
        loadContext={vi.fn().mockResolvedValue(mergeContext)}
        onChoose={onChoose}
        onSaveMerged={vi.fn()}
        onClose={onClose}
      />,
    );

    fireEvent.click(await screen.findByRole("button", { name: "Keep existing Source" }));
    await waitFor(() => {
      expect(onChoose).toHaveBeenCalledWith("item-a", {
        kind: "keep_current_source",
        sourceId: "source-internal",
        candidateHash: "candidate-hash",
        currentHash: "current-hash",
        targetVersionId: "version-internal",
      });
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("stages the edited merged result before marking the item ready", async () => {
    const onSaveMerged = vi.fn().mockResolvedValue(undefined);
    render(
      <ImportMergeResolutionDialog
        open
        itemId="item-a"
        title="notes.md"
        loadContext={vi.fn().mockResolvedValue(mergeContext)}
        onChoose={vi.fn()}
        onSaveMerged={onSaveMerged}
        onClose={vi.fn()}
      />,
    );

    const editor = await screen.findByRole("textbox", { name: "Merged result" });
    fireEvent.change(editor, { target: { value: "# 合并结果\n\n保留双方内容" } });
    fireEvent.click(screen.getByRole("button", { name: "Use merged result" }));

    await waitFor(() => {
      expect(onSaveMerged).toHaveBeenCalledWith(
        "item-a",
        "# 合并结果\n\n保留双方内容",
      );
    });
  });
});
